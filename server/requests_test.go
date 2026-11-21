package main

// The stop requests document in store mode (D-159): a write goes through
// the receive path — cap-requests validates it and answers its canonical
// bytes, a stale write is refused, the manifest pins the head, and a raw
// pointer commit is refused.

import (
	"context"
	"encoding/json"
	"net/http/httptest"
	"strings"
	"testing"

	"rustwasm/server/store"
)

func TestStopRequestsWritesAreValidated(t *testing.T) {
	s := newStoreServer(t, store.NewMemory())
	closeModules(t, s)
	mux := storeMux(s)
	mux.HandleFunc("/stop-requests", s.handleKeptRoute)
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	base := srv.URL

	post := func(from string, doc any) (int, map[string]any) {
		body, _ := json.Marshal(map[string]any{"from": from, "requests": doc})
		code, _, resp := httpPost(t, base+"/stop-requests", string(body))
		var out map[string]any
		json.Unmarshal([]byte(resp), &out)
		return code, out
	}
	bad := map[string]any{"requests": map[string]any{"R-1": map[string]any{"stop": "A", "job": "new", "status": "ordered"}}}
	if code, out := post("", bad); code != 422 {
		t.Fatalf("an ordered request with no work order is refused: %d %v", code, out)
	}
	good := map[string]any{"requests": map[string]any{"R-1": map[string]any{"stop": "A", "job": "new", "status": "submitted", "requested_on": "2026-09-01"}}}
	code, out := post("", good)
	if code != 200 {
		t.Fatalf("a request: %d %v", code, out)
	}
	hash := out["requests_hash"].(string)
	_, _, body := httpGet(t, base+"/manifest")
	var m struct {
		StopRequestsHash string `json:"stop_requests_hash"`
	}
	json.Unmarshal([]byte(body), &m)
	if m.StopRequestsHash != hash {
		t.Fatalf("the manifest pins the requests: %q vs %q", m.StopRequestsHash, hash)
	}
	code, hdr, blob := httpGet(t, base+"/blobs/"+hash)
	if code != 200 || hdr.Get("X-Document-Kind") != "stop_requests" || sha([]byte(blob)) != hash {
		t.Fatalf("the requests blob: %d kind=%q", code, hdr.Get("X-Document-Kind"))
	}
	if code, out := post("", good); code != 409 {
		t.Fatalf("a write from a version that is no longer the head is refused: %d %v", code, out)
	}
	raw, _ := json.Marshal(map[string]any{"from": hash, "document": "{}"})
	if code, _, _ := httpPost(t, base+"/pointers/stop_requests", string(raw)); code != 400 {
		t.Fatalf("the requests are written only through POST /stop-requests: %d", code)
	}
}

// The work order system through its adapter (D-165): with none
// configured the server answers 501 and the manifest offers nothing (a
// request's work order is entered by hand); with the stub, opening a work
// order for a request writes its reference on it — the request ordered,
// the writer the system — and reading its status writes the status and
// the day it was read.
func TestWorkOrdersGoThroughTheAdapter(t *testing.T) {
	s := newStoreServer(t, store.NewMemory())
	closeModules(t, s)
	s.fixedToday = "2026-09-02"
	mux := storeMux(s)
	mux.HandleFunc("/stop-requests", s.handleKeptRoute)
	mux.HandleFunc("/stop-requests/work-order", s.handleWorkOrder)
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	base := srv.URL

	ask := func(action string) (int, map[string]any) {
		code, _, resp := httpPost(t, base+"/stop-requests/work-order", `{"request":"R-1","action":"`+action+`"}`)
		var out map[string]any
		json.Unmarshal([]byte(resp), &out)
		return code, out
	}
	services := func() string {
		_, _, body := httpGet(t, base+"/manifest")
		var m struct {
			Services map[string]any `json:"services"`
		}
		json.Unmarshal([]byte(body), &m)
		raw, _ := json.Marshal(m.Services)
		return string(raw)
	}
	if code, out := ask("open"); code != 501 || out["configured"] != false {
		t.Fatalf("no adapter: %d %v", code, out)
	}
	if strings.Contains(services(), "work_orders") {
		t.Fatalf("no adapter, nothing offered: %s", services())
	}

	var err error
	if s.workOrders, err = newWorkOrders("stub:"); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(services(), "/stop-requests/work-order") {
		t.Fatalf("the adapter is offered: %s", services())
	}
	body, _ := json.Marshal(map[string]any{"from": "", "requests": map[string]any{"requests": map[string]any{"R-1": map[string]any{"stop": "A", "job": "new", "status": "submitted"}}}})
	if code, _, resp := httpPost(t, base+"/stop-requests", string(body)); code != 200 {
		t.Fatalf("a request: %d %s", code, resp)
	}
	code, out := ask("open")
	if code != 200 {
		t.Fatalf("open: %d %v", code, out)
	}
	stored := func(hash string) map[string]any {
		blob := logBlob(t, s, hash)
		var doc struct {
			Requests map[string]map[string]any `json:"requests"`
		}
		json.Unmarshal(blob, &doc)
		return doc.Requests["R-1"]
	}
	r := stored(out["requests_hash"].(string))
	wo := r["work_order"].(map[string]any)
	if r["status"] != "ordered" || wo["id"] != "STUB-R-1" || wo["system"] != "stub" || wo["read_on"] != "2026-09-02" {
		t.Fatalf("the work order on the request: %v", r)
	}
	if w := lastLogWriter(t, s, "stops"); w != "work-orders:stub" {
		t.Fatalf("the writer is the system: %q", w)
	}
	s.fixedToday = "2026-09-09"
	code, out = ask("refresh")
	if code != 200 {
		t.Fatalf("refresh: %d %v", code, out)
	}
	if wo := stored(out["requests_hash"].(string))["work_order"].(map[string]any); wo["status"] != "open" || wo["read_on"] != "2026-09-09" || wo["id"] != "STUB-R-1" {
		t.Fatalf("the status read: %v", wo)
	}
	if _, err := newWorkOrders("https://wo.example"); err == nil {
		t.Fatal("an adapter not written yet is refused at start, never guessed")
	}
}

// countingWorkOrders counts the work orders the system is asked to open.
type countingWorkOrders struct {
	workOrderSystem
	opened int
}

func (c *countingWorkOrders) Open(ctx context.Context, id string, request json.RawMessage) (workOrder, error) {
	c.opened++
	return c.workOrderSystem.Open(ctx, id, request)
}

// An outbox's resend of a work order whose answer was lost (D-189): its
// key landed, so the server answers the work order the request holds,
// and the system is not asked to open a second one.
func TestAWorkOrderResentWithItsKeyOpensOnce(t *testing.T) {
	s := newStoreServer(t, store.NewMemory())
	closeModules(t, s)
	stub, err := newWorkOrders("stub:")
	if err != nil {
		t.Fatal(err)
	}
	system := &countingWorkOrders{workOrderSystem: stub}
	s.workOrders = system
	mux := storeMux(s)
	mux.HandleFunc("/stop-requests", s.handleKeptRoute)
	mux.HandleFunc("/stop-requests/work-order", s.handleWorkOrder)
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	body, _ := json.Marshal(map[string]any{"from": "", "requests": map[string]any{"requests": map[string]any{"R-1": map[string]any{"stop": "A", "job": "new", "status": "submitted"}}}})
	if code, _, resp := httpPost(t, srv.URL+"/stop-requests", string(body)); code != 200 {
		t.Fatalf("a request: %d %s", code, resp)
	}
	open := `{"request":"R-1","action":"open","key":"stops/tablet-7"}`
	code, _, first := httpPost(t, srv.URL+"/stop-requests/work-order", open)
	if code != 200 {
		t.Fatalf("open: %d %s", code, first)
	}
	code, _, again := httpPost(t, srv.URL+"/stop-requests/work-order", open)
	var out map[string]any
	json.Unmarshal([]byte(again), &out)
	wo, _ := out["work_order"].(map[string]any)
	if code != 200 || out["landed"] != true || wo["id"] != "STUB-R-1" || system.opened != 1 {
		t.Fatalf("the resend lands once, the system asked once: %d %s (opened %d)", code, again, system.opened)
	}
}

// A write answers the broken references it leaves (D-233): a stop
// request naming a stop the stops log does not hold lands, with the
// reference named as a warning for a person, and the entry it is in.
func TestAWriteAnswersTheBrokenReferencesItLeaves(t *testing.T) {
	s := newStoreServer(t, store.NewMemory())
	closeModules(t, s)
	mux := storeMux(s)
	mux.HandleFunc("/stop-requests", s.handleKeptRoute)
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)

	doc := map[string]any{"requests": map[string]any{"R-1": map[string]any{"stop": "NO-SUCH-STOP", "job": "new", "status": "submitted", "requested_on": "2026-09-01"}}}
	body, _ := json.Marshal(map[string]any{"from": "", "requests": doc})
	code, _, resp := httpPost(t, srv.URL+"/stop-requests", string(body))
	var out struct {
		RequestsHash string `json:"requests_hash"`
		References   []struct {
			Log, Kind, ID, Field, Target, Names string
			Entry                               struct {
				Log string
				Seq int64
			}
		} `json:"references"`
	}
	json.Unmarshal([]byte(resp), &out)
	if code != 200 || out.RequestsHash == "" {
		t.Fatalf("a broken reference never refuses the write: %d %s", code, resp)
	}
	if len(out.References) != 1 {
		t.Fatalf("the write's one broken reference: %s", resp)
	}
	r := out.References[0]
	if r.Log != "stops" || r.Kind != "stop_request" || r.ID != "R-1" || r.Field != "/stop" || r.Target != "stop" || r.Names != "NO-SUCH-STOP" || r.Entry.Log != "stops" || r.Entry.Seq < 1 {
		t.Fatalf("the warning names the record, its field, the target, and the entry: %+v", r)
	}
}
