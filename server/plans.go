package main

// Publishing a pick (store mode): POST /plans takes a plan's network
// document — any serialization, gzipped or not — and makes it the
// pick every client resolves (docs/ROADMAP.md 1). The host decides
// nothing:
//
//   - cap-network (a fresh instance, closed after) registers the
//     document, validates it (a network with issues never publishes),
//     and answers its canonical bytes, hash and plan frame;
//   - cap-plans answers the next published plan set — or the reasons
//     the pick would break the chain — as canonical bytes and hash.
//
// The host stores exactly those bytes: the network under the
// plan.<id> pointer (its log keeps every revision referenced, so GC
// never collects a published network), then the set under the plans
// pointer, which the manifest pins as plans_hash. A client reads the
// set by hash, asks cap-plans in_force for the date, and loads that
// network by hash from /blobs.
//
//	POST /plans          body: the network document
//	                     200 {plan_id, network_hash, plans_hash, warnings}
//	                     422 {issues, refusals} (an invalid network, a pick that
//	                         runs no service in its period, or a pick the chain
//	                         refuses — one dated before today, D-76)
//	                     400 (not a network; a plan id that is not a pointer name)
//	GET  /feed?date=     the GTFS zip in force on the date (default today):
//	                     cap-network's export of the pick in force, or the
//	                     -feed zip before the first pick takes effect
//	POST /plans/retire   body: {"id"} — retire a pick that has not taken
//	                     effect (D-75): 200 {plans_hash}, 422 {issues}, 404
//	                     when nothing is published
//
// "Today" is the host's: the server's local date, or -today on a test
// server. The modules decide what it means.

import (
	"context"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"time"

	"rustwasm/server/store"
)

// plansEnvelope is the envelope of every published plan set: produced
// by the hosted cap-plans module.
func (s *server) plansEnvelope() store.Envelope {
	return store.Envelope{Kind: "plans", SchemaVersion: 1, ProducedBy: "cap-plans@" + s.plans.Hash()}
}

// canonicalPick runs a received network through a fresh cap-network:
// register (kernel op 256; gzip or plain), what refuses it as a pick
// (`publishable`: its issues and refusing warnings, answered as the
// 422 body), and the canonical document with its plan frame. The
// instance closes with the call.
func canonicalPick(ctx context.Context, wasmPath string, body []byte) (doc canonicalNetwork, issues json.RawMessage, err error) {
	host, err := NewWasmHost(ctx, wasmPath)
	if err != nil {
		return doc, nil, fmt.Errorf("network module: %w", err)
	}
	defer host.Close(ctx)
	var loaded struct {
		NetworkHash string `json:"network_hash"`
	}
	if err := host.InvokeBytes(ctx, opLoadNetwork, body, &loaded); err != nil {
		return doc, nil, fmt.Errorf("not a network document: %w", err)
	}
	var checked struct {
		Issues   []json.RawMessage `json:"issues"`
		Refusals []json.RawMessage `json:"refusals"`
		Warnings json.RawMessage   `json:"warnings"`
	}
	if err := host.Invoke(ctx, map[string]any{"op": "publishable", "network_hash": loaded.NetworkHash}, &checked); err != nil {
		return doc, nil, err
	}
	if len(checked.Issues)+len(checked.Refusals) > 0 {
		refused, _ := json.Marshal(map[string]any{"issues": checked.Issues, "refusals": checked.Refusals})
		return doc, refused, nil
	}
	if err := host.Invoke(ctx, map[string]any{"op": "document", "network_hash": loaded.NetworkHash}, &doc); err != nil {
		return doc, nil, err
	}
	doc.Warnings = checked.Warnings
	doc.ProducedBy = "cap-network@" + host.Hash()
	return doc, nil, nil
}

// canonicalNetwork is cap-network's document answer: the bytes, their
// hash, and the plan frame (passed through to cap-plans unread).
type canonicalNetwork struct {
	Document     string          `json:"document"`
	DocumentHash string          `json:"document_hash"`
	Plan         json.RawMessage `json:"plan"`
	Timezone     string          `json:"timezone"`
	Warnings     json.RawMessage `json:"-"`
	ProducedBy   string          `json:"-"`
}

func (s *server) handlePlans(w http.ResponseWriter, r *http.Request) {
	if s.store == nil || s.plans == nil {
		http.Error(w, "publishing needs the store (start with -store <url>)", http.StatusNotFound)
		return
	}
	if r.Method != http.MethodPost {
		http.Error(w, "use POST (clients read the published set through the manifest's plans_hash)", http.StatusMethodNotAllowed)
		return
	}
	body, err := io.ReadAll(io.LimitReader(r.Body, 128<<20))
	if err != nil {
		http.Error(w, "read body: "+err.Error(), http.StatusBadRequest)
		return
	}
	pick, issues, err := canonicalPick(r.Context(), s.networkWasm, body)
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	if issues != nil {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusUnprocessableEntity)
		w.Write(issues)
		return
	}
	var frame struct {
		ID string `json:"id"`
	}
	json.Unmarshal(pick.Plan, &frame)
	// The pick names the agency's zone: today below is its date.
	if pick.Timezone != "" {
		s.zoneMu.Lock()
		s.zone = pick.Timezone
		s.zoneMu.Unlock()
	}

	// One publish at a time: the set is read, extended and committed
	// as one step (the CAS below still guards against any other writer).
	s.plansMu.Lock()
	defer s.plansMu.Unlock()
	head, found, err := s.store.Head("plans")
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	var current json.RawMessage = json.RawMessage("null")
	if found {
		if current, _, err = s.store.GetBlob(head.Hash); err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
	}
	var next struct {
		Issues       json.RawMessage `json:"issues"`
		Document     *string         `json:"document"`
		DocumentHash *string         `json:"document_hash"`
	}
	if err := s.plans.Invoke(r.Context(), map[string]any{
		"op": "publish", "published": current, "plan": pick.Plan, "network_hash": pick.DocumentHash, "today": s.today(),
	}, &next); err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	if next.Document == nil {
		writeJSON(w, http.StatusUnprocessableEntity, map[string]any{"issues": next.Issues})
		return
	}
	pointer := "plan." + frame.ID
	if !store.ValidPointerName(pointer) {
		http.Error(w, fmt.Sprintf("plan id %q cannot name a pointer (letters, digits, '.', '_', '-')", frame.ID), http.StatusBadRequest)
		return
	}

	writer := writerOf(r, "anonymous")
	nowMs := time.Now().UnixMilli()
	// The network first: once it is committed it is referenced, and a
	// crash before the set's commit leaves the old set in force.
	netEnv := store.Envelope{Kind: "network_document", SchemaVersion: networkVersion, ProducedBy: pick.ProducedBy}
	planHead, planFound, _ := s.store.Head(pointer)
	planFrom := ""
	if planFound {
		planFrom = planHead.Hash
	}
	if _, _, err := s.store.CAS(pointer, planFrom, []byte(pick.Document), netEnv, pick.DocumentHash, writer, nowMs); err != nil {
		http.Error(w, err.Error(), http.StatusConflict)
		return
	}
	from := ""
	if found {
		from = head.Hash
	}
	plansHash, _, err := s.store.CAS("plans", from, []byte(*next.Document), s.plansEnvelope(), *next.DocumentHash, writer, nowMs)
	if err != nil {
		http.Error(w, err.Error(), http.StatusConflict)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{
		"plan_id": frame.ID, "network_hash": pick.DocumentHash, "plans_hash": plansHash, "warnings": pick.Warnings,
	})
}

// today is the agency's date, "YYYY-MM-DD": the host's clock
// read in the network's zone — the feed's, else the last published
// pick's, as cap-network answered it — never the host's own zone, so a
// server in a UTC container answers the agency's service day. With no
// zone known yet (no feed, nothing published since boot) it is the
// host's local date. -today pins it on a test server.
func (s *server) today() string {
	if s.fixedToday != "" {
		return s.fixedToday
	}
	now := time.Now()
	if s.now != nil {
		now = s.now()
	}
	if loc := s.agencyZone(); loc != nil {
		now = now.In(loc)
	}
	return now.Format("2006-01-02")
}

// agencyZone is the zone today() counts in: the published feed's, else
// the last published pick's; nil when neither is known or the name is
// not a zone the host's tz database has.
func (s *server) agencyZone() *time.Location {
	zone := ""
	if s.feeder != nil {
		if feed, err := s.feeder.Load(); err == nil {
			zone = feed.timezone
		}
	}
	if zone == "" {
		s.zoneMu.Lock()
		zone = s.zone
		s.zoneMu.Unlock()
	}
	if zone == "" {
		return nil
	}
	loc, err := time.LoadLocation(zone)
	if err != nil {
		log.Printf("agency timezone %q: %v (counting today in the host's zone)", zone, err)
		return nil
	}
	return loc
}

// handleRetire retires a pick (D-75): cap-plans answers the next set —
// the pick kept as the tombstone — or why not, and the host commits it.
func (s *server) handleRetire(w http.ResponseWriter, r *http.Request) {
	if s.store == nil || s.plans == nil {
		http.Error(w, "publishing needs the store (start with -store <url>)", http.StatusNotFound)
		return
	}
	if r.Method != http.MethodPost {
		http.Error(w, "use POST", http.StatusMethodNotAllowed)
		return
	}
	var req struct {
		ID string `json:"id"`
	}
	if err := json.NewDecoder(io.LimitReader(r.Body, 1<<20)).Decode(&req); err != nil || req.ID == "" {
		http.Error(w, `body: {"id": "<plan id>"}`, http.StatusBadRequest)
		return
	}
	s.plansMu.Lock()
	defer s.plansMu.Unlock()
	head, found, err := s.store.Head("plans")
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	if !found {
		http.Error(w, "nothing is published", http.StatusNotFound)
		return
	}
	current, _, err := s.store.GetBlob(head.Hash)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	var next struct {
		Issues       json.RawMessage `json:"issues"`
		Document     *string         `json:"document"`
		DocumentHash *string         `json:"document_hash"`
	}
	if err := s.plans.Invoke(r.Context(), map[string]any{
		"op": "retire", "published": json.RawMessage(current), "id": req.ID, "today": s.today(),
	}, &next); err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	if next.Document == nil {
		writeJSON(w, http.StatusUnprocessableEntity, map[string]any{"issues": next.Issues})
		return
	}
	plansHash, _, err := s.store.CAS("plans", head.Hash, []byte(*next.Document), s.plansEnvelope(), *next.DocumentHash, writerOf(r, "anonymous"), time.Now().UnixMilli())
	if err != nil {
		http.Error(w, err.Error(), http.StatusConflict)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"plans_hash": plansHash})
}

// handleFeed serves the GTFS feed in force on a date (?date=, default
// today): the zip cap-network `export` writes for the published pick
// governing it — cap-plans `in_force` answers which — or, before the
// first pick takes effect, the configured -feed zip itself (D-74).
// 404 when neither runs. The export runs in a fresh cap-network and is
// kept by the pick's network hash: a pick's zip is one set of bytes.
func (s *server) handleFeed(w http.ResponseWriter, r *http.Request) {
	date := r.URL.Query().Get("date")
	if date == "" {
		date = s.today()
	}
	if _, err := time.Parse("2006-01-02", date); err != nil {
		http.Error(w, "date: YYYY-MM-DD", http.StatusBadRequest)
		return
	}
	var pick *struct {
		ID          string `json:"id"`
		NetworkHash string `json:"network_hash"`
	}
	if s.store != nil && s.plans != nil {
		if head, found, err := s.store.Head("plans"); err == nil && found {
			set, _, err := s.store.GetBlob(head.Hash)
			if err != nil {
				http.Error(w, err.Error(), http.StatusInternalServerError)
				return
			}
			var ans struct {
				Plan *struct {
					ID          string `json:"id"`
					NetworkHash string `json:"network_hash"`
				} `json:"plan"`
			}
			if err := s.plans.Invoke(r.Context(), map[string]any{"op": "in_force", "published": json.RawMessage(set), "date": date}, &ans); err != nil {
				http.Error(w, err.Error(), http.StatusBadRequest)
				return
			}
			pick = ans.Plan
		}
	}
	var zipBytes []byte
	name := "gtfs.zip"
	switch {
	case pick != nil:
		var err error
		if zipBytes, err = s.exportedPick(r.Context(), pick.NetworkHash); err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		name = "gtfs-" + pick.ID + ".zip"
	case s.feeder != nil:
		var err error
		if zipBytes, err = os.ReadFile(s.feeder.path); err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
	default:
		http.Error(w, "no feed is in force on "+date, http.StatusNotFound)
		return
	}
	sum := sha256.Sum256(zipBytes)
	etag := `"sha256:` + hex.EncodeToString(sum[:]) + `"`
	w.Header().Set("ETag", etag)
	if r.Header.Get("If-None-Match") == etag {
		w.WriteHeader(http.StatusNotModified)
		return
	}
	w.Header().Set("Content-Type", "application/zip")
	w.Header().Set("Content-Disposition", `attachment; filename="`+name+`"`)
	w.Write(zipBytes)
}

// exportedPick is cap-network's export of a published pick's network,
// by network hash (kept for the process: the bytes never change).
func (s *server) exportedPick(ctx context.Context, networkHash string) ([]byte, error) {
	s.feedMu.Lock()
	defer s.feedMu.Unlock()
	if zipBytes, ok := s.feedCache[networkHash]; ok {
		return zipBytes, nil
	}
	blob, _, err := s.store.GetBlob(networkHash)
	if err != nil {
		return nil, err
	}
	host, err := NewWasmHost(ctx, s.networkWasm)
	if err != nil {
		return nil, fmt.Errorf("network module: %w", err)
	}
	defer host.Close(ctx)
	var loaded struct {
		NetworkHash string `json:"network_hash"`
	}
	if err := host.InvokeBytes(ctx, opLoadNetwork, blob, &loaded); err != nil {
		return nil, err
	}
	var exported struct {
		FeedHash string `json:"feed_hash"`
		ZipB64   string `json:"zip_b64"`
	}
	if err := host.Invoke(ctx, map[string]any{"op": "export", "network_hash": loaded.NetworkHash}, &exported); err != nil {
		return nil, err
	}
	zipBytes, err := base64.StdEncoding.DecodeString(exported.ZipB64)
	if err != nil {
		return nil, err
	}
	if s.feedCache == nil || len(s.feedCache) >= 4 {
		s.feedCache = map[string][]byte{}
	}
	s.feedCache[networkHash] = zipBytes
	return zipBytes, nil
}

func writeJSON(w http.ResponseWriter, code int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(code)
	json.NewEncoder(w).Encode(v)
}
