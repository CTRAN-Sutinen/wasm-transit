package main

// The stop requests document (store mode; D-159): the planning team's
// requests for work at a stop, each tracking the work order the
// agency's work order system opens for it. The host decides nothing: a
// write goes through the receive path — cap-requests validate answers
// every refusal (422) or the canonical bytes and hash the host stores
// under the stop_requests pointer; a write naming a version that is no
// longer the head is refused (409), and the writer re-reads.
//
// One cap-requests instance serves every use, emptied after each
// (`withRequests`).
//
//	POST /stop-requests  body: {"from": "<the head the writer read, or empty>", "requests": {...}}
//	                     200 {requests_hash}
//	                     422 {refusals}
//	                     409 {current, seq}
//
// The manifest pins the head as stop_requests_hash; a page reads the
// document by hash from /blobs.

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
	requestsPointer    = "stop_requests"
	opLoadStopRequests = 271
)

func (s *server) requestsEnvelope(producedBy string) store.Envelope {
	return store.Envelope{Kind: "stop_requests", SchemaVersion: 1, ProducedBy: producedBy}
}

// withRequests runs `use` on the server's one cap-requests instance —
// made on first use — and then empties it (the kernel's evict), so no
// document outlives the request that sent it.
func (s *server) withRequests(ctx context.Context, use func(*WasmHost) error) error {
	if s.requestsWasm == "" {
		return fmt.Errorf("no requests module (start with -requests-wasm)")
	}
	s.requestsMu.Lock()
	defer s.requestsMu.Unlock()
	if s.requestsHeld == nil {
		host, err := NewWasmHost(ctx, s.requestsWasm)
		if err != nil {
			return fmt.Errorf("requests module: %w", err)
		}
		s.requestsHeld = host
	}
	err := use(s.requestsHeld)
	var evicted json.RawMessage
	if evictErr := s.requestsHeld.InvokeBytes(ctx, opEvict, []byte(`{"keep":[]}`), &evicted); evictErr != nil && err == nil {
		err = fmt.Errorf("emptying the requests module: %w", evictErr)
	}
	return err
}

func (s *server) handleStopRequests(w http.ResponseWriter, r *http.Request) {
	if s.store == nil {
		http.Error(w, "the stop requests document needs the store (start with -store <url>)", http.StatusNotFound)
		return
	}
	if r.Method != http.MethodPost {
		http.Error(w, "use POST (clients read the stop requests document through the manifest's stop_requests_hash)", http.StatusMethodNotAllowed)
		return
	}
	var req struct {
		From     string          `json:"from"`
		Requests json.RawMessage `json:"requests"`
	}
	body, err := io.ReadAll(io.LimitReader(r.Body, 64<<20))
	if err == nil {
		err = json.Unmarshal(body, &req)
	}
	if err != nil || len(req.Requests) == 0 {
		http.Error(w, `body: {"from": "<hash or empty>", "requests": {...}}`, http.StatusBadRequest)
		return
	}
	ctx := r.Context()
	var checked struct {
		Refusals     json.RawMessage `json:"refusals"`
		Document     *string         `json:"document"`
		DocumentHash *string         `json:"document_hash"`
	}
	var producedBy string
	err = s.withRequests(ctx, func(host *WasmHost) error {
		producedBy = "cap-requests@" + host.Hash()
		return host.Invoke(ctx, map[string]any{"op": "validate", "requests": req.Requests}, &checked)
	})
	if err != nil {
		fail(w, err, http.StatusBadRequest)
		return
	}
	if checked.Document == nil {
		writeJSON(w, http.StatusUnprocessableEntity, map[string]any{"refusals": checked.Refusals})
		return
	}
	hash, _, err := s.store.CAS(requestsPointer, req.From, []byte(*checked.Document), s.requestsEnvelope(producedBy), *checked.DocumentHash, writerOf(r, "anonymous"), time.Now().UnixMilli())
	if err != nil {
		var ce *store.ConflictError
		if asConflict(err, &ce) {
			writeJSON(w, http.StatusConflict, map[string]any{"current": ce.Current, "seq": ce.Seq})
			return
		}
		fail(w, err, http.StatusInternalServerError)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"requests_hash": hash})
}
