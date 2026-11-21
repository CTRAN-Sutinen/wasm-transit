package main

// The work order system (D-165): the agency's own system that crews work
// from, reached through an adapter so nothing here couples to it. A stop
// request (D-159) tracks the work order it opens; the adapter opens one
// for a request and reads its status back, and what it answers is
// written onto the request as one entry of the stops log, through its
// receive op (`requests record_work_order`, checked as any write;
// logkinds.go), the writer `work-orders:<system>`.
//
// The call splits in three (D-228): cap-requests `work_order_call` answers
// what to send, this host makes the call, and the stops log's receive op
// records the answer. The host reads nothing out of the document.
//
// -work-orders names the adapter: unset, none is configured — the
// manifest offers nothing and a request's work order is entered by
// hand; `stub:` (or `stub:<name>`) a stand-in that opens STUB-<request>
// and reports it open, for tests and demos. The real system's adapter
// is written from its OpenAPI spec and slots in behind the same
// interface.
//
//	POST /stop-requests/work-order  body: {"request": "<id>", "action": "open"|"refresh", "key": "<idempotency key>"}
//	                                200 {requests_hash, work_order, landed?}
//	                                501 {configured: false}   no adapter
//	                                404 no such request · 409 the requests moved · 422 {refusals}

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"

	"rustwasm/server/store"
)

// workOrder is the work order system's answer, in the request's terms.
type workOrder struct {
	System string `json:"system,omitempty"`
	ID     string `json:"id"`
	URL    string `json:"url,omitempty"`
	Status string `json:"status,omitempty"`
	ReadOn string `json:"read_on,omitempty"`
}

// workOrderSystem is an adapter to the agency's work order system.
type workOrderSystem interface {
	// Name is the system's name, as a request records it.
	Name() string
	// Open asks the system for a work order for the request (its id and
	// its document as the stop requests document holds it).
	Open(ctx context.Context, requestID string, request json.RawMessage) (workOrder, error)
	// Status reads a work order's status from the system.
	Status(ctx context.Context, held workOrder) (workOrder, error)
}

// newWorkOrders reads -work-orders: nil when none is configured.
func newWorkOrders(spec string) (workOrderSystem, error) {
	switch {
	case spec == "":
		return nil, nil
	case spec == "stub:" || strings.HasPrefix(spec, "stub:"):
		name := strings.TrimPrefix(spec, "stub:")
		if name == "" {
			name = "stub"
		}
		return stubWorkOrders{name: name}, nil
	default:
		return nil, fmt.Errorf("-work-orders %q: no adapter for it yet (the real one is written from the system's OpenAPI spec; stub: is the stand-in)", spec)
	}
}

// stubWorkOrders is the stand-in adapter: STUB-<request>, open.
type stubWorkOrders struct{ name string }

func (w stubWorkOrders) Name() string { return w.name }

func (w stubWorkOrders) Open(_ context.Context, requestID string, _ json.RawMessage) (workOrder, error) {
	return workOrder{System: w.name, ID: "STUB-" + requestID, Status: "open"}, nil
}

func (w stubWorkOrders) Status(_ context.Context, held workOrder) (workOrder, error) {
	if held.ID == "" {
		return workOrder{}, errors.New("the request has no work order to read")
	}
	return workOrder{System: w.name, ID: held.ID, Status: "open"}, nil
}

func (s *server) handleWorkOrder(w http.ResponseWriter, r *http.Request) {
	if s.workOrders == nil {
		writeJSON(w, http.StatusNotImplemented, map[string]any{"configured": false})
		return
	}
	if s.store == nil {
		http.Error(w, "the stop requests document needs the store (start with -store <url>)", http.StatusNotFound)
		return
	}
	if r.Method != http.MethodPost {
		http.Error(w, "use POST", http.StatusMethodNotAllowed)
		return
	}
	var req struct {
		Request string `json:"request"`
		Action  string `json:"action"`
		Key     string `json:"key"`
	}
	body, err := io.ReadAll(io.LimitReader(r.Body, 1<<16))
	if err == nil {
		err = json.Unmarshal(body, &req)
	}
	if err != nil || req.Request == "" || (req.Action != "open" && req.Action != "refresh") {
		http.Error(w, `body: {"request": "<id>", "action": "open"|"refresh"}`, http.StatusBadRequest)
		return
	}
	ctx := r.Context()
	l := s.keptLogOf("stop_requests")
	if l == nil {
		http.Error(w, "the kind table names no log for the stop requests", http.StatusNotFound)
		return
	}
	_, doc, found, err := s.keptHead("stop_requests")
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return
	}
	if !found {
		http.Error(w, "no stop requests yet", http.StatusNotFound)
		return
	}
	// cap-requests answers what to send the system for the request: the
	// request for an open, the work order it holds for a refresh.
	var call struct {
		Call struct {
			Request   json.RawMessage `json:"request"`
			WorkOrder *workOrder      `json:"work_order"`
		} `json:"call"`
	}
	err = s.withModule(ctx, "requests", func(host *WasmHost) error {
		var loaded map[string]any
		if err := host.InvokeBytes(ctx, l.loadCode("stop_requests"), doc, &loaded); err != nil {
			return err
		}
		return host.Invoke(ctx, map[string]any{"op": "work_order_call", "requests_hash": loadedHash(loaded), "id": req.Request, "action": req.Action}, &call)
	})
	if err != nil {
		http.Error(w, err.Error(), http.StatusNotFound)
		return
	}
	held := call.Call
	// An outbox's resend (D-189): a key that landed answers the work
	// order the request holds, and the system is not asked again, so a
	// lost answer never opens a second work order.
	if req.Key != "" {
		f := l.fold
		f.mu.Lock()
		_, landed, err := s.keyLanded(f, req.Key)
		hash := l.docs["stop_requests"].hash
		f.mu.Unlock()
		if err != nil {
			fail(w, err, http.StatusInternalServerError)
			return
		}
		if landed {
			writeJSON(w, http.StatusOK, map[string]any{"requests_hash": hash, "work_order": held.WorkOrder, "landed": true})
			return
		}
	}
	var answer workOrder
	if req.Action == "open" {
		answer, err = s.workOrders.Open(ctx, req.Request, held.Request)
	} else {
		var ref workOrder
		if held.WorkOrder != nil {
			ref = *held.WorkOrder
		}
		answer, err = s.workOrders.Status(ctx, ref)
	}
	if err != nil {
		fail(w, fmt.Errorf("the work order system: %w", err), http.StatusBadGateway)
		return
	}
	if answer.System == "" {
		answer.System = s.workOrders.Name()
	}
	answer.ReadOn = s.today()
	// The answer is recorded over the requests at the head when the write
	// runs, so a request written meanwhile is kept.
	op, _ := json.Marshal(map[string]any{"cap": "requests", "op": "record_work_order", "args": map[string]any{"id": req.Request, "work_order": answer}})
	write := logWrite{writer: "work-orders:" + s.workOrders.Name(), op: "POST /stop-requests/work-order", argsHash: store.HashOf(body), key: req.Key}
	_, _, got, refusal, err := s.receiveLog(ctx, r, l, write, receiveCall{ops: []json.RawMessage{op}})
	if err != nil {
		logWriteFailed(w, err, http.StatusBadRequest)
		return
	}
	if refusal != nil {
		if got.Failed != nil {
			http.Error(w, got.Failed.Error, http.StatusBadRequest)
			return
		}
		if body, ok := refusal.body.(map[string]any); ok {
			delete(body, "op")
		}
		writeRefused(w, refusal)
		return
	}
	l.fold.mu.Lock()
	hash := l.docs["stop_requests"].hash
	l.fold.mu.Unlock()
	writeJSON(w, http.StatusOK, map[string]any{"requests_hash": hash, "work_order": answer})
}
