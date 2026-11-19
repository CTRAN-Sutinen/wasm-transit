package main

// The stop requests document (store mode; D-159): the planning team's
// requests for work at a stop, each tracking the work order the
// agency's work order system opens for it. The host decides nothing: a
// write goes through the receive path — cap-requests validate answers
// every refusal (422) or the canonical document, which the stops log
// records as one entry (stopslog.go); a write naming a version that is
// no longer the head is refused (409), and the writer re-reads.
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

	"rustwasm/server/store"
)

const (
	requestsPointer    = "stop_requests"
	opLoadStopRequests = 271
)

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
	writer := writerOf(r, "anonymous")
	var unknown []string
	f := s.stopsLog()
	_, _, refusal, err := s.writeLog(f, logWrite{writer: writer, op: "POST /stop-requests", argsHash: store.HashOf(body)}, func() ([]json.RawMessage, *logRefusal, error) {
		if moved := fromMoved(req.From, s.requestsDoc, f.seq); moved != nil {
			return nil, moved, nil
		}
		var records []json.RawMessage
		var refusal *logRefusal
		err := s.withRequests(ctx, func(host *WasmHost) error {
			var checked struct {
				Refusals      json.RawMessage `json:"refusals"`
				Document      *string         `json:"document"`
				UnknownFields []string        `json:"unknown_fields"`
			}
			if err := host.Invoke(ctx, map[string]any{"op": "validate", "requests": req.Requests}, &checked); err != nil {
				return err
			}
			if checked.Document == nil {
				refusal = &logRefusal{http.StatusUnprocessableEntity, map[string]any{"refusals": checked.Refusals}}
				return nil
			}
			unknown = checked.UnknownFields
			var err error
			records, err = s.requestRecords(ctx, host, []byte(*checked.Document))
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
	flagUnknown(w, "stop requests", writer, unknown)
	f.mu.Lock()
	hash := s.requestsDoc.hash
	f.mu.Unlock()
	writeJSON(w, http.StatusOK, map[string]any{"requests_hash": hash})
}
