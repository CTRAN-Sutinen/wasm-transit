package main

// The stops document (store mode; docs/units/stop-lifecycle.md; D-153,
// D-156): every stop a list of dated events, beside the picks. The
// host decides nothing:
//
// One cap-stops instance serves every use, emptied after each
// (`withStops`).
//
//   - a write goes through the receive path: cap-stops validate answers
//     every refusal (422) or the canonical bytes and hash the host
//     stores under the stops pointer; a write naming a version that is
//     no longer the head is refused (409), and the writer re-reads;
//   - a pick's stops are a copy: publishing a pick refreshes it from the
//     stops document for its window (cap-stops refresh), and a stops
//     write republishes every pick in force or to come whose stops it
//     changes — its network under plan.<id>, then the set cap-plans
//     answers.
//
// A server with no stops document publishes exactly as before.
//
//	POST /stops  body: {"from": "<the head the writer read, or empty>", "stops": {...}}
//	             200 {stops_hash, republished: [plan ids]}
//	             422 {refusals}
//	             409 {current, seq}
//
// The manifest pins the head as stops_hash; a page reads the document
// by hash from /blobs.

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"

	"rustwasm/server/store"
)

const (
	stopsPointer = "stops"
	opLoadStops  = 270
	opEvict      = 257
)

func (s *server) stopsEnvelope(producedBy string) store.Envelope {
	return store.Envelope{Kind: "stops", SchemaVersion: 1, ProducedBy: producedBy}
}

// withStops runs `use` on the server's one cap-stops instance — made on
// first use, the module compiled once — and then empties it (the
// kernel's evict, keeping nothing), so no network or stops document
// outlives the request that sent it. One use at a time: a use is a
// sequence of calls (loads, then the op).
func (s *server) withStops(ctx context.Context, use func(*WasmHost) error) error {
	if s.stopsWasm == "" {
		return fmt.Errorf("no stops module (start with -stops-wasm)")
	}
	s.stopsMu.Lock()
	defer s.stopsMu.Unlock()
	if s.stopsHeld == nil {
		host, err := NewWasmHost(ctx, s.stopsWasm)
		if err != nil {
			return fmt.Errorf("stops module: %w", err)
		}
		s.stopsHeld = host
	}
	err := use(s.stopsHeld)
	var evicted json.RawMessage
	if evictErr := s.stopsHeld.InvokeBytes(ctx, opEvict, []byte(`{"keep":[]}`), &evicted); evictErr != nil && err == nil {
		err = fmt.Errorf("emptying the stops module: %w", evictErr)
	}
	return err
}

// stopsHead is the stops document in force: its hash and bytes, or
// found false when there is none.
func (s *server) stopsHead() (hash string, doc []byte, found bool, err error) {
	head, found, err := s.store.Head(stopsPointer)
	if err != nil || !found {
		return "", nil, found, err
	}
	doc, _, err = s.store.GetBlob(head.Hash)
	return head.Hash, doc, true, err
}

// refreshed is a pick's network refreshed from the stops document for
// [from, until): cap-stops registers both and answers the network's
// canonical bytes and hash, and the module that made them.
func (s *server) refreshed(ctx context.Context, network, stops []byte, plan, from string, until *string) (doc []byte, hash, producedBy string, err error) {
	err = s.withStops(ctx, func(host *WasmHost) error {
		var loadedNet struct {
			NetworkHash string `json:"network_hash"`
		}
		if err := host.InvokeBytes(ctx, opLoadNetwork, network, &loadedNet); err != nil {
			return fmt.Errorf("the pick's network: %w", err)
		}
		var loadedStops struct {
			StopsHash string `json:"stops_hash"`
		}
		if err := host.InvokeBytes(ctx, opLoadStops, stops, &loadedStops); err != nil {
			return fmt.Errorf("the stops document: %w", err)
		}
		req := map[string]any{"op": "refresh", "network_hash": loadedNet.NetworkHash, "stops_hash": loadedStops.StopsHash, "from": from, "plan": plan}
		if until != nil {
			req["until"] = *until
		}
		var out struct {
			NetworkHash string          `json:"network_hash"`
			Network     json.RawMessage `json:"network"`
		}
		if err := host.Invoke(ctx, req, &out); err != nil {
			return err
		}
		doc, hash, producedBy = out.Network, out.NetworkHash, "cap-stops@"+host.Hash()
		return nil
	})
	return doc, hash, producedBy, err
}

// publishedEntry is one pick in the published set, as the host passes
// it along unread but for its frame.
type publishedEntry struct {
	ID            string `json:"id"`
	Name          string `json:"name"`
	EffectiveFrom string `json:"effective_from"`
	NetworkHash   string `json:"network_hash"`
	Status        string `json:"status,omitempty"`
}

func publishedEntries(set json.RawMessage) []publishedEntry {
	var parsed struct {
		Plans []publishedEntry `json:"plans"`
	}
	json.Unmarshal(set, &parsed)
	return parsed.Plans
}

// windows is each pick's governing period as cap-plans derives it over
// the published picks (retired ones govern nothing), with `extra`
// placed by id: id → until (nil = open-ended).
func (s *server) windows(ctx context.Context, set json.RawMessage, extra *publishedEntry) (map[string]*string, error) {
	var infos []map[string]any
	for _, p := range publishedEntries(set) {
		if p.Status == "retired" || (extra != nil && p.ID == extra.ID) {
			continue
		}
		infos = append(infos, map[string]any{"id": p.ID, "name": p.Name, "effective_from": p.EffectiveFrom})
	}
	if extra != nil {
		infos = append(infos, map[string]any{"id": extra.ID, "name": extra.Name, "effective_from": extra.EffectiveFrom})
	}
	var out struct {
		Periods []struct {
			ID    string  `json:"id"`
			Until *string `json:"until"`
		} `json:"periods"`
	}
	if infos == nil {
		infos = []map[string]any{}
	}
	if err := s.plans.Invoke(ctx, map[string]any{"op": "periods", "plans": infos}, &out); err != nil {
		return nil, err
	}
	windows := map[string]*string{}
	for _, p := range out.Periods {
		windows[p.ID] = p.Until
	}
	return windows, nil
}

func (s *server) handleStops(w http.ResponseWriter, r *http.Request) {
	if s.store == nil || s.plans == nil {
		http.Error(w, "the stops document needs the store (start with -store <url>)", http.StatusNotFound)
		return
	}
	if r.Method != http.MethodPost {
		http.Error(w, "use POST (clients read the stops document through the manifest's stops_hash)", http.StatusMethodNotAllowed)
		return
	}
	var req struct {
		From  string          `json:"from"`
		Stops json.RawMessage `json:"stops"`
	}
	body, err := io.ReadAll(io.LimitReader(r.Body, 64<<20))
	if err == nil {
		err = json.Unmarshal(body, &req)
	}
	if err != nil || len(req.Stops) == 0 {
		http.Error(w, `body: {"from": "<hash or empty>", "stops": {...}}`, http.StatusBadRequest)
		return
	}
	ctx := r.Context()
	var checked struct {
		Refusals     json.RawMessage `json:"refusals"`
		Document     *string         `json:"document"`
		DocumentHash *string         `json:"document_hash"`
	}
	var producedBy string
	err = s.withStops(ctx, func(host *WasmHost) error {
		producedBy = "cap-stops@" + host.Hash()
		return host.Invoke(ctx, map[string]any{"op": "validate", "stops": req.Stops}, &checked)
	})
	if err != nil {
		fail(w, err, http.StatusBadRequest)
		return
	}
	if checked.Document == nil {
		writeJSON(w, http.StatusUnprocessableEntity, map[string]any{"refusals": checked.Refusals})
		return
	}

	// The stops and the picks they refresh move together: one write at
	// a time (each CAS below still guards against any other writer).
	s.plansMu.Lock()
	defer s.plansMu.Unlock()
	writer := writerOf(r, "anonymous")
	nowMs := time.Now().UnixMilli()
	hash, _, err := s.store.CAS(stopsPointer, req.From, []byte(*checked.Document), s.stopsEnvelope(producedBy), *checked.DocumentHash, writer, nowMs)
	if err != nil {
		var ce *store.ConflictError
		if asConflict(err, &ce) {
			writeJSON(w, http.StatusConflict, map[string]any{"current": ce.Current, "seq": ce.Seq})
			return
		}
		fail(w, err, http.StatusInternalServerError)
		return
	}
	republished, err := s.republish(ctx, []byte(*checked.Document), writer)
	if err != nil {
		// The stops are stored; a pick not yet refreshed is refreshed by
		// the next write or publish.
		fail(w, fmt.Errorf("stops stored as %s, republishing the picks failed: %w", hash, err), http.StatusInternalServerError)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"stops_hash": hash, "republished": republished})
}

// republish refreshes every published pick in force or to come from
// the stops document, and republishes each whose network changed. The
// caller holds plansMu.
func (s *server) republish(ctx context.Context, stops []byte, writer string) ([]string, error) {
	republished := []string{}
	head, found, err := s.store.Head("plans")
	if err != nil || !found {
		return republished, err
	}
	set, _, err := s.store.GetBlob(head.Hash)
	if err != nil {
		return nil, err
	}
	windows, err := s.windows(ctx, set, nil)
	if err != nil {
		return nil, err
	}
	today := s.today()
	current, currentHash := json.RawMessage(set), head.Hash
	for _, p := range publishedEntries(set) {
		until, governs := windows[p.ID]
		if p.Status == "retired" || !governs || (until != nil && *until <= today) {
			continue // retired, or its period has ended
		}
		network, _, err := s.store.GetBlob(p.NetworkHash)
		if err != nil {
			return nil, err
		}
		doc, hash, producedBy, err := s.refreshed(ctx, network, stops, p.ID, p.EffectiveFrom, until)
		if err != nil {
			return nil, fmt.Errorf("pick %s: %w", p.ID, err)
		}
		if hash == p.NetworkHash {
			continue
		}
		var next struct {
			Issues       json.RawMessage `json:"issues"`
			Document     *string         `json:"document"`
			DocumentHash *string         `json:"document_hash"`
		}
		plan := map[string]any{"id": p.ID, "name": p.Name, "effective_from": p.EffectiveFrom}
		if err := s.plans.Invoke(ctx, map[string]any{"op": "publish", "published": current, "plan": plan, "network_hash": hash, "today": today}, &next); err != nil {
			return nil, err
		}
		if next.Document == nil {
			return nil, fmt.Errorf("pick %s: the chain refuses its refreshed network: %s", p.ID, next.Issues)
		}
		nowMs := time.Now().UnixMilli()
		env := store.Envelope{Kind: "network_document", SchemaVersion: networkVersion, ProducedBy: producedBy}
		if _, _, err := s.store.CAS("plan."+p.ID, p.NetworkHash, doc, env, hash, writer, nowMs); err != nil {
			return nil, fmt.Errorf("pick %s: %w", p.ID, err)
		}
		current = json.RawMessage(*next.Document)
		currentHash = *next.DocumentHash
		republished = append(republished, p.ID)
	}
	if len(republished) > 0 {
		if _, _, err := s.store.CAS("plans", head.Hash, current, s.plansEnvelope(), currentHash, writer, time.Now().UnixMilli()); err != nil {
			return nil, err
		}
	}
	return republished, nil
}

// refreshPick refreshes a pick being published from the stops document
// for its window over the published set (`set`, "null" before the
// first publish); nothing changes without a stops document or a pick
// date (an undated pick never publishes).
func (s *server) refreshPick(ctx context.Context, pick *canonicalNetwork, set json.RawMessage) error {
	if s.stopsWasm == "" {
		return nil
	}
	var frame publishedEntry
	json.Unmarshal(pick.Plan, &frame)
	if frame.EffectiveFrom == "" {
		return nil
	}
	_, stops, found, err := s.stopsHead()
	if err != nil || !found {
		return err
	}
	windows, err := s.windows(ctx, set, &frame)
	if err != nil {
		return err
	}
	doc, hash, producedBy, err := s.refreshed(ctx, []byte(pick.Document), stops, frame.ID, frame.EffectiveFrom, windows[frame.ID])
	if err != nil {
		return err
	}
	if hash != pick.DocumentHash {
		pick.Document, pick.DocumentHash, pick.ProducedBy = string(doc), hash, producedBy
	}
	return nil
}
