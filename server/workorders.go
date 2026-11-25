package main

// The work order system is the agency's own system, reached
// through an adapter so nothing here couples to it. The call splits in
// three: the kind's `call` op answers what to send, this host
// makes the call, and its `record` op, through the kind's log
// (logkinds.go), records the answer under the writer
// `work-orders:<system>`. The kind table's `work_order` names the kind,
// the route, and both ops. The host reads nothing out of the document.

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

type workOrder struct {
	System string `json:"system,omitempty"`
	ID     string `json:"id"`
	URL    string `json:"url,omitempty"`
	Status string `json:"status,omitempty"`
	ReadOn string `json:"read_on,omitempty"`
}

type workOrderSystem interface {
	Name() string
	Open(ctx context.Context, requestID string, request json.RawMessage) (workOrder, error)
	Status(ctx context.Context, held workOrder) (workOrder, error)
}

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
	e := s.workOrderKind()
	if e == nil {
		http.NotFound(w, r)
		return
	}
	if s.store == nil {
		http.Error(w, "the "+e.StoreKind+" document needs the store (start with -store <url>)", http.StatusNotFound)
		return
	}
	if r.Method != e.WorkOrder.Method {
		http.Error(w, "use "+e.WorkOrder.Method, http.StatusMethodNotAllowed)
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
	l := s.keptLogOf(e.StoreKind)
	if l == nil {
		http.Error(w, "the kind table names no log for "+e.StoreKind, http.StatusNotFound)
		return
	}
	hashField := e.hashField()
	_, doc, found, err := s.keptHead(e.StoreKind)
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return
	}
	if !found {
		http.Error(w, "no "+e.StoreKind+" yet", http.StatusNotFound)
		return
	}
	// The checking capability answers what to send: the record for an
	// open, the held work order for a refresh.
	var call struct {
		Call struct {
			Request   json.RawMessage `json:"request"`
			WorkOrder *workOrder      `json:"work_order"`
		} `json:"call"`
	}
	err = s.withModule(ctx, e.Module, func(host *WasmHost) error {
		var loaded map[string]any
		if err := host.InvokeBytes(ctx, l.loadCode(e.StoreKind), doc, &loaded); err != nil {
			return err
		}
		return host.Invoke(ctx, map[string]any{"op": e.WorkOrder.Call, hashField: loadedHash(loaded), "id": req.Request, "action": req.Action}, &call)
	})
	if err != nil {
		http.Error(w, err.Error(), http.StatusNotFound)
		return
	}
	held := call.Call
	// An outbox's resend with a key that landed gets the held
	// work order without asking the system, so a lost answer never opens
	// a second work order.
	if req.Key != "" {
		f := l.fold
		f.mu.Lock()
		_, landed, err := s.keyLanded(f, req.Key)
		hash := l.docs[e.StoreKind].hash
		f.mu.Unlock()
		if err != nil {
			fail(w, err, http.StatusInternalServerError)
			return
		}
		if landed {
			writeJSON(w, http.StatusOK, map[string]any{hashField: hash, "work_order": held.WorkOrder, "landed": true})
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
	// The write runs over the records at the head, so a record written
	// meanwhile is kept.
	op, _ := json.Marshal(map[string]any{"cap": e.Module, "op": e.WorkOrder.Record, "args": map[string]any{"id": req.Request, "work_order": answer}})
	write := logWrite{writer: "work-orders:" + s.workOrders.Name(), op: e.WorkOrder.Method + " " + e.WorkOrder.Path, argsHash: store.HashOf(body), key: req.Key}
	_, _, got, refusal, err := s.receiveLog(ctx, r, l, write, receiveCall{ops: []json.RawMessage{op}})
	if err != nil {
		logWriteFailed(w, err, http.StatusBadRequest)
		return
	}
	if refusal != nil {
		if got.Failed != nil {
			answerFailed(w, got.Failed.Error, http.StatusBadRequest, "")
			return
		}
		if body, ok := refusal.body.(map[string]any); ok {
			delete(body, "op")
		}
		writeRefused(w, refusal)
		return
	}
	l.fold.mu.Lock()
	hash := l.docs[e.StoreKind].hash
	l.fold.mu.Unlock()
	writeJSON(w, http.StatusOK, map[string]any{hashField: hash, "work_order": answer})
}
