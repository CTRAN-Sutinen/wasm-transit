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

const requestsWasm = "../web/pkg/cap_requests.wasm"

func TestStopRequestsWritesAreValidated(t *testing.T) {
	s := newStoreServer(t, store.NewMemory())
	s.requestsWasm = requestsWasm
	t.Cleanup(func() {
		if s.requestsHeld != nil {
			s.requestsHeld.Close(context.Background())
		}
	})
	mux := storeMux(s)
	mux.HandleFunc("/stop-requests", s.handleStopRequests)
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
	s.requestsWasm = requestsWasm
	t.Cleanup(func() {
		if s.requestsHeld != nil {
			s.requestsHeld.Close(context.Background())
		}
	})
	s.fixedToday = "2026-09-02"
	mux := storeMux(s)
	mux.HandleFunc("/stop-requests", s.handleStopRequests)
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
		blob, _, err := s.store.GetBlob(hash)
		if err != nil {
			t.Fatal(err)
		}
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
	entries, _, _ := s.store.Log(requestsPointer, "")
	if w := entries[len(entries)-1].Writer; w != "work-orders:stub" {
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
