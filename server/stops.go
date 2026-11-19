package main

// The stops document (store mode; D-153, D-157): every stop a list of
// dated events, its own lifecycle managed apart from the picks, which
// only read it. The host decides nothing: a write goes through the
// receive path — cap-stops validate answers every refusal (422) or the
// canonical document, which the stops log records as one entry
// (stopslog.go); a write naming a version that is no longer the head is
// refused (409), and the writer re-reads. A stops write changes no pick. A pick being
// published reads it (`readPick`): each stop's record on the pick's
// first date (cap-stops read_records), and the warnings of the stops
// it visits that are not in service over its period (visit_warnings).
//
// One cap-stops instance serves every use, emptied after each
// (`withStops`).
//
//	POST /stops  body: {"from": "<the head the writer read, or empty>", "stops": {...}}
//	             200 {stops_hash}
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
	"log"
	"net/http"

	"rustwasm/server/store"
)

const (
	stopsPointer = "stops"
	opLoadStops  = 270
	opEvict      = 257
)

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

// readPick is what a pick being published reads of the stops document
// (D-157), over the published set `set` ("null" before the first
// publish): each stop's record on its first date, the pick rewritten
// when that changes it, and the warnings of the stops it visits that
// are not in service over its period. Nothing without a stops document
// or a pick date; the stops document is never written.
func (s *server) readPick(ctx context.Context, pick *canonicalNetwork, set json.RawMessage) ([]json.RawMessage, error) {
	if s.stopsWasm == "" {
		return nil, nil
	}
	var frame publishedEntry
	json.Unmarshal(pick.Plan, &frame)
	if frame.EffectiveFrom == "" {
		return nil, nil
	}
	_, stops, found, err := s.stopsHead()
	if err != nil || !found {
		return nil, err
	}
	windows, err := s.windows(ctx, set, &frame)
	if err != nil {
		return nil, err
	}
	var warnings []json.RawMessage
	err = s.withStops(ctx, func(host *WasmHost) error {
		var loadedNet struct {
			NetworkHash string `json:"network_hash"`
		}
		if err := host.InvokeBytes(ctx, opLoadNetwork, []byte(pick.Document), &loadedNet); err != nil {
			return fmt.Errorf("the pick's network: %w", err)
		}
		var loadedStops struct {
			StopsHash string `json:"stops_hash"`
		}
		if err := host.InvokeBytes(ctx, opLoadStops, stops, &loadedStops); err != nil {
			return fmt.Errorf("the stops document: %w", err)
		}
		var read struct {
			NetworkHash string          `json:"network_hash"`
			Network     json.RawMessage `json:"network"`
		}
		if err := host.Invoke(ctx, map[string]any{"op": "read_records", "network_hash": loadedNet.NetworkHash, "stops_hash": loadedStops.StopsHash, "on": frame.EffectiveFrom}, &read); err != nil {
			return err
		}
		req := map[string]any{"op": "visit_warnings", "network_hash": read.NetworkHash, "stops_hash": loadedStops.StopsHash, "from": frame.EffectiveFrom}
		if until := windows[frame.ID]; until != nil {
			req["until"] = *until
		}
		var warned struct {
			Warnings []json.RawMessage `json:"warnings"`
		}
		if err := host.Invoke(ctx, req, &warned); err != nil {
			return err
		}
		warnings = warned.Warnings
		if read.NetworkHash != pick.DocumentHash {
			pick.Document, pick.DocumentHash, pick.ProducedBy = string(read.Network), read.NetworkHash, "cap-stops@"+host.Hash()
		}
		return nil
	})
	return warnings, err
}

func (s *server) handleStops(w http.ResponseWriter, r *http.Request) {
	if s.store == nil {
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
	writer := writerOf(r, "anonymous")
	by := nameOf(r, "anonymous")
	var unknown []string
	f := s.stopsLog()
	_, _, refusal, err := s.writeLog(f, logWrite{writer: writer, op: "POST /stops", argsHash: store.HashOf(body)}, func() ([]json.RawMessage, *logRefusal, error) {
		if moved := fromMoved(req.From, s.stopsDoc, f.seq); moved != nil {
			return nil, moved, nil
		}
		var records []json.RawMessage
		var refusal *logRefusal
		err := s.withStops(ctx, func(host *WasmHost) error {
			// The surveyor is the signed-in writer (D-158): every survey
			// the held document does not hold names them, whatever name
			// the client sent; then the write is checked.
			checked, flagged, ref, err := s.checkStops(ctx, host, req.Stops, s.stopsDoc.doc, by)
			if err != nil || ref != nil {
				refusal = ref
				return err
			}
			unknown = flagged
			records, err = s.stopRecords(ctx, host, checked)
			return err
		})
		if err != nil || refusal != nil {
			return nil, refusal, err
		}
		return records, nil, nil
	})
	if err != nil {
		logWriteFailed(w, err, http.StatusBadRequest)
		return
	}
	if refusal != nil {
		writeJSON(w, refusal.status, refusal.body)
		return
	}
	flagUnknown(w, "stops", writer, unknown)
	f.mu.Lock()
	hash := s.stopsDoc.hash
	f.mu.Unlock()
	writeJSON(w, http.StatusOK, map[string]any{"stops_hash": hash})
}

// recordFeed records the agency's feed in the stops document (D-150):
// each difference between the feed's stops and the document on the
// feed's first service date an event dated that day, naming the feed
// (cap-stops import_events with the feed's hash), written as one entry
// of the stops log like any write, the writer `import:feed`. The planner is shown
// what it did (the stops page's notice, cap-stops feed_events). Nothing
// without a store, the stops module, or a stops document; nothing when
// the feed differs in nothing.
func (s *server) recordFeed(ctx context.Context, feed *loadedFeed) error {
	if s.store == nil || s.stopsWasm == "" {
		return nil
	}
	if _, _, found, err := s.stopsHead(); err != nil || !found {
		return err
	}
	var added int
	_, _, refusal, err := s.writeLog(s.stopsLog(), logWrite{writer: "import:feed", op: "import_events", argsHash: feed.hash}, func() ([]json.RawMessage, *logRefusal, error) {
		var records []json.RawMessage
		var refusal *logRefusal
		err := s.withStops(ctx, func(host *WasmHost) error {
			var loadedNet struct {
				NetworkHash string `json:"network_hash"`
			}
			if err := host.InvokeBytes(ctx, opLoadNetwork, feed.document, &loadedNet); err != nil {
				return fmt.Errorf("the feed's network: %w", err)
			}
			var loadedStops struct {
				StopsHash string `json:"stops_hash"`
			}
			if err := host.InvokeBytes(ctx, opLoadStops, s.stopsDoc.doc, &loadedStops); err != nil {
				return fmt.Errorf("the stops document: %w", err)
			}
			var imported struct {
				Stops json.RawMessage   `json:"stops"`
				Added []json.RawMessage `json:"added"`
			}
			if err := host.Invoke(ctx, map[string]any{"op": "import_events", "stops_hash": loadedStops.StopsHash, "network_hash": loadedNet.NetworkHash, "feed": feed.hash}, &imported); err != nil {
				return err
			}
			added = len(imported.Added)
			if added == 0 {
				return nil
			}
			var checked struct {
				Refusals json.RawMessage `json:"refusals"`
				Document *string         `json:"document"`
			}
			if err := host.Invoke(ctx, map[string]any{"op": "validate", "stops": imported.Stops}, &checked); err != nil {
				return err
			}
			if checked.Document == nil {
				refusal = &logRefusal{http.StatusUnprocessableEntity, checked.Refusals}
				return nil
			}
			var err error
			records, err = s.stopRecords(ctx, host, []byte(*checked.Document))
			return err
		})
		if err != nil || refusal != nil || added == 0 {
			return nil, refusal, err
		}
		return records, nil, nil
	})
	if err != nil {
		return err
	}
	if refusal != nil {
		return fmt.Errorf("the recorded document is refused: %v", refusal.body)
	}
	if added > 0 {
		log.Printf("stops: the feed %s recorded %d change(s)", feed.hash, added)
	}
	return nil
}
