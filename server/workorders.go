package main

// The work order system (D-165): the agency's own system that crews work
// from, reached through an adapter so nothing here couples to it. A stop
// request (D-159) tracks the work order it opens; the adapter opens one
// for a request and reads its status back, and what it answers is
// written onto the request through cap-requests (`record_work_order`,
// checked as any write) under the stop_requests pointer, the writer
// `work-orders:<system>`.
//
// -work-orders names the adapter: unset, none is configured — the
// manifest offers nothing and a request's work order is entered by
// hand; `stub:` (or `stub:<name>`) a stand-in that opens STUB-<request>
// and reports it open, for tests and demos. The real system's adapter
// is written from its OpenAPI spec and slots in behind the same
// interface.
//
//	POST /stop-requests/work-order  body: {"request": "<id>", "action": "open"|"refresh"}
//	                                200 {requests_hash, work_order}
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
	"time"

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
	head, found, err := s.store.Head(requestsPointer)
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return
	}
	if !found {
		http.Error(w, "no stop requests yet", http.StatusNotFound)
		return
	}
	doc, _, err := s.store.GetBlob(head.Hash)
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return
	}
	var parsed struct {
		Requests map[string]struct {
			WorkOrder *workOrder `json:"work_order"`
		} `json:"requests"`
	}
	var raw struct {
		Requests map[string]json.RawMessage `json:"requests"`
	}
	json.Unmarshal(doc, &parsed)
	json.Unmarshal(doc, &raw)
	held, ok := parsed.Requests[req.Request]
	if !ok {
		http.Error(w, "no request "+req.Request, http.StatusNotFound)
		return
	}
	var answer workOrder
	if req.Action == "open" {
		answer, err = s.workOrders.Open(ctx, req.Request, raw.Requests[req.Request])
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
	var recorded struct {
		Refusals     json.RawMessage `json:"refusals"`
		RequestsHash *string         `json:"requests_hash"`
		Requests     json.RawMessage `json:"requests"`
	}
	var checked struct {
		Document     *string `json:"document"`
		DocumentHash *string `json:"document_hash"`
	}
	var producedBy string
	err = s.withRequests(ctx, func(host *WasmHost) error {
		producedBy = "cap-requests@" + host.Hash()
		var loaded struct {
			RequestsHash string `json:"requests_hash"`
		}
		if err := host.InvokeBytes(ctx, opLoadStopRequests, doc, &loaded); err != nil {
			return err
		}
		if err := host.Invoke(ctx, map[string]any{"op": "record_work_order", "requests_hash": loaded.RequestsHash, "id": req.Request, "work_order": answer}, &recorded); err != nil {
			return err
		}
		if recorded.RequestsHash == nil {
			return nil
		}
		return host.Invoke(ctx, map[string]any{"op": "validate", "requests": recorded.Requests}, &checked)
	})
	if err != nil {
		fail(w, err, http.StatusBadRequest)
		return
	}
	if checked.Document == nil {
		writeJSON(w, http.StatusUnprocessableEntity, map[string]any{"refusals": recorded.Refusals})
		return
	}
	hash, _, err := s.store.CAS(requestsPointer, head.Hash, []byte(*checked.Document), s.requestsEnvelope(producedBy), *checked.DocumentHash, "work-orders:"+s.workOrders.Name(), time.Now().UnixMilli())
	if err != nil {
		var ce *store.ConflictError
		if asConflict(err, &ce) {
			writeJSON(w, http.StatusConflict, map[string]any{"current": ce.Current, "seq": ce.Seq})
			return
		}
		fail(w, err, http.StatusInternalServerError)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"requests_hash": hash, "work_order": answer})
}
