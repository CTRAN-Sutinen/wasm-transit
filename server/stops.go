package main

// The stops document (store mode; D-153, D-157): every stop a list of
// dated events, its own lifecycle managed apart from the picks, which
// only read it. The host decides nothing: a write goes through the
// receive path — cap-stops validate answers every refusal (422) or the
// canonical bytes and hash the host stores under the stops pointer; a
// write naming a version that is no longer the head is refused (409),
// and the writer re-reads. A stops write changes no pick.
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
	"net/http"
	"time"

	"rustwasm/server/store"
)

const (
	stopsPointer = "stops"
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
	writeJSON(w, http.StatusOK, map[string]any{"stops_hash": hash})
}
