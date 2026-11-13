package main

// The stops document in store mode (D-147, D-152, D-156): a write goes
// through the receive path — cap-stops validates it and answers its
// canonical bytes, a stale write is refused — and a pick's stops are a
// copy refreshed from the document: publishing refreshes the pick for
// its window and opens the stops its plan proposed, and a stop change
// republishes every pick in force or to come whose stops it changes. A server with no stops document publishes exactly as before.

import (
	"context"
	"encoding/json"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"

	"rustwasm/server/store"
)

const stopsWasm = "../web/pkg/cap_stops.wasm"

func newStopsServer(t *testing.T, b store.Backend) (*server, string) {
	t.Helper()
	s := newStoreServer(t, b)
	s.plans = newPlansHost(t)
	s.networkWasm = networkWasm
	s.stopsWasm = stopsWasm
	s.fixedToday = "2026-08-01"
	t.Cleanup(func() {
		if s.stopsHeld != nil {
			s.stopsHeld.Close(context.Background())
		}
	})
	mux := storeMux(s)
	mux.HandleFunc("/plans", s.handlePlans)
	mux.HandleFunc("/plans/retire", s.handleRetire)
	mux.HandleFunc("/stops", s.handleStops)
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	return s, srv.URL
}

// seededStops is the fixture network's stops as a stops document, made
// the way a page makes it: cap-stops seed over the network — once per
// test binary (each instance costs seconds under -race).
var (
	seededOnce sync.Once
	seededRaw  []byte
	seededErr  error
)

func seededStops(t *testing.T) map[string]any {
	t.Helper()
	fixtureNetwork(t, "x", "x", "")
	seededOnce.Do(func() {
		host, err := NewWasmHost(context.Background(), stopsWasm)
		if err != nil {
			seededErr = err
			return
		}
		defer host.Close(context.Background())
		var loaded struct {
			NetworkHash string `json:"network_hash"`
		}
		if seededErr = host.InvokeBytes(context.Background(), opLoadNetwork, fixtureDoc, &loaded); seededErr != nil {
			return
		}
		var seeded struct {
			Stops json.RawMessage `json:"stops"`
		}
		seededErr = host.Invoke(context.Background(), map[string]any{"op": "seed", "network_hash": loaded.NetworkHash}, &seeded)
		seededRaw = seeded.Stops
	})
	if seededErr != nil {
		t.Fatal(seededErr)
	}
	var doc map[string]any
	json.Unmarshal(seededRaw, &doc)
	return doc
}

// withClosure is the document with stop A closed from `on` until `until`.
func withClosure(doc map[string]any, on, until string) map[string]any {
	raw, _ := json.Marshal(doc)
	var out map[string]any
	json.Unmarshal(raw, &out)
	stops := out["stops"].(map[string]any)
	stops["A"] = append(stops["A"].([]any), map[string]any{"on": on, "event": "closed", "until": until})
	return out
}

func postStops(t *testing.T, base, from string, doc any) (int, map[string]any) {
	t.Helper()
	body, _ := json.Marshal(map[string]any{"from": from, "stops": doc})
	code, _, resp := httpPost(t, base+"/stops", string(body))
	var out map[string]any
	json.Unmarshal([]byte(resp), &out)
	if out == nil {
		out = map[string]any{"raw": resp}
	}
	return code, out
}

func manifestStopsHash(t *testing.T, base string) string {
	t.Helper()
	_, _, body := httpGet(t, base+"/manifest")
	var m struct {
		StopsHash string `json:"stops_hash"`
	}
	json.Unmarshal([]byte(body), &m)
	return m.StopsHash
}

func pickHashes(t *testing.T, s *server) map[string]string {
	t.Helper()
	head, found, err := s.store.Head("plans")
	if err != nil || !found {
		t.Fatalf("plans head: %v %v", found, err)
	}
	blob, _, err := s.store.GetBlob(head.Hash)
	if err != nil {
		t.Fatal(err)
	}
	var set publishedSet
	json.Unmarshal(blob, &set)
	out := map[string]string{}
	for _, p := range set.Plans {
		out[p.ID] = p.NetworkHash
	}
	return out
}

func TestStopsWritesAreValidated(t *testing.T) {
	_, base := newStopsServer(t, store.NewMemory())
	if h := manifestStopsHash(t, base); h != "" {
		t.Fatalf("no stops document yet, the manifest pins none: %q", h)
	}
	bad := map[string]any{"stops": map[string]any{"a": []any{map[string]any{"event": "opened", "lat": 45, "lon": -122}, map[string]any{"on": "2026-09-01", "event": "reopened"}}}}
	if code, out := postStops(t, base, "", bad); code != 422 || !strings.Contains(out["refusals"].(any).([]any)[0].(map[string]any)["problem"].(map[string]any)["kind"].(string), "nothing_closed") {
		t.Fatalf("a refused document: %d %v", code, out)
	}
	seed := seededStops(t)
	code, out := postStops(t, base, "", seed)
	if code != 200 {
		t.Fatalf("the seed: %d %v", code, out)
	}
	hash, _ := out["stops_hash"].(string)
	if manifestStopsHash(t, base) != hash || hash == "" {
		t.Fatalf("the manifest pins the stops document: %q vs %v", manifestStopsHash(t, base), out)
	}
	code, hdr, body := httpGet(t, base+"/blobs/"+hash)
	if code != 200 || hdr.Get("X-Document-Kind") != "stops" || sha([]byte(body)) != hash {
		t.Fatalf("the stops blob: %d kind=%q", code, hdr.Get("X-Document-Kind"))
	}
	if code, out := postStops(t, base, "", withClosure(seed, "2026-10-23", "2026-10-24")); code != 409 {
		t.Fatalf("a write from a version that is no longer the head is refused: %d %v", code, out)
	}
	raw, _ := json.Marshal(map[string]any{"from": hash, "document": "{}"})
	if code, _, _ := httpPost(t, base+"/pointers/stops", string(raw)); code != 400 {
		t.Fatalf("the stops document is written only through POST /stops: %d", code)
	}
}

func TestAStopChangeRefreshesThePicksItTouches(t *testing.T) {
	s, base := newStopsServer(t, store.NewMemory())
	if code, out := publishPick(t, base, fixtureNetwork(t, "fall-2026", "Fall 2026", "2026-09-07")); code != 200 {
		t.Fatalf("publish fall: %d %v", code, out)
	}
	winter := fixtureNetwork(t, "winter-2026", "Winter 2026", "2026-12-01")
	winter["name"] = "winter network"
	if code, out := publishPick(t, base, winter); code != 200 {
		t.Fatalf("publish winter: %d %v", code, out)
	}
	before := pickHashes(t, s)

	// the seed changes no pick: each network's stops are the document's
	seed := seededStops(t)
	code, out := postStops(t, base, "", seed)
	if code != 200 {
		t.Fatalf("the seed: %d %v", code, out)
	}
	if got := pickHashes(t, s); got["fall-2026"] != before["fall-2026"] || got["winter-2026"] != before["winter-2026"] {
		t.Fatalf("a seed that matches every pick republishes none: %v -> %v", before, got)
	}

	// A closed inside fall's window: fall is republished, winter is not
	code, out = postStops(t, base, out["stops_hash"].(string), withClosure(seed, "2026-10-23", "2026-10-24"))
	if code != 200 {
		t.Fatalf("the closure: %d %v", code, out)
	}
	if r, _ := out["republished"].([]any); len(r) != 1 || r[0] != "fall-2026" {
		t.Fatalf("only the pick whose window holds the closure republishes: %v", out)
	}
	after := pickHashes(t, s)
	if after["fall-2026"] == before["fall-2026"] || after["winter-2026"] != before["winter-2026"] {
		t.Fatalf("fall's network changes, winter's does not: %v -> %v", before, after)
	}
	_, _, body := httpGet(t, base+"/blobs/"+after["fall-2026"])
	var net struct {
		Stops map[string]struct {
			Events []map[string]any `json:"events"`
		} `json:"stops"`
	}
	json.Unmarshal([]byte(body), &net)
	if len(net.Stops["A"].Events) != 2 || len(net.Stops["B"].Events) != 0 {
		t.Fatalf("A carries its closure, B nothing: %+v", net.Stops)
	}

	// publishing refreshes too: a pick whose window holds the closure
	// is stored with it, whatever copy the planner sent
	spring := fixtureNetwork(t, "fall-2026", "Fall 2026", "2026-09-07")
	spring["name"] = "fall, corrected"
	code, out = publishPick(t, base, spring)
	if code != 200 {
		t.Fatalf("republish fall: %d %v", code, out)
	}
	_, _, body = httpGet(t, base+"/blobs/"+out["network_hash"].(string))
	json.Unmarshal([]byte(body), &net)
	if len(net.Stops["A"].Events) != 2 {
		t.Fatalf("a published pick carries the stops document's closure: %+v", net.Stops["A"])
	}
}

func TestPublishingAPlanOpensTheStopsItProposed(t *testing.T) {
	s, base := newStopsServer(t, store.NewMemory())
	seed := seededStops(t)
	raw, _ := json.Marshal(seed)
	var doc map[string]any
	json.Unmarshal(raw, &doc)
	doc["stops"].(map[string]any)["N"] = []any{map[string]any{"on": "2026-08-15", "event": "proposed", "plan": "fall-2026", "lat": 45.5, "lon": -122.6, "name": "New stop"}}
	code, out := postStops(t, base, "", doc)
	if code != 200 {
		t.Fatalf("the proposal: %d %v", code, out)
	}
	before := out["stops_hash"].(string)

	code, out = publishPick(t, base, fixtureNetwork(t, "fall-2026", "Fall 2026", "2026-09-07"))
	if code != 200 {
		t.Fatalf("publish fall: %d %v", code, out)
	}
	if o, _ := out["opened"].([]any); len(o) != 1 || o[0] != "N" {
		t.Fatalf("the publish answers the stop it opened: %v", out)
	}
	after := manifestStopsHash(t, base)
	if after == before || after == "" {
		t.Fatalf("the stops document is written: %q -> %q", before, after)
	}
	_, _, body := httpGet(t, base+"/blobs/"+after)
	var stored struct {
		Stops map[string][]map[string]any `json:"stops"`
	}
	json.Unmarshal([]byte(body), &stored)
	if n := stored.Stops["N"]; len(n) != 2 || n[1]["event"] != "opened" || n[1]["on"] != "2026-09-07" {
		t.Fatalf("N opens on the plan's first date: %v", n)
	}
	_, _, body = httpGet(t, base+"/blobs/"+pickHashes(t, s)["fall-2026"])
	var net struct {
		Stops map[string]struct {
			Events []map[string]any `json:"events"`
		} `json:"stops"`
	}
	json.Unmarshal([]byte(body), &net)
	if len(net.Stops["N"].Events) != 2 {
		t.Fatalf("the pick carries N, open from its first date: %+v", net.Stops["N"])
	}

	// publishing again opens nothing more and leaves the stops alone
	again := fixtureNetwork(t, "fall-2026", "Fall 2026", "2026-09-07")
	again["name"] = "fall, again"
	code, out = publishPick(t, base, again)
	if code != 200 || out["opened"] != nil || manifestStopsHash(t, base) != after {
		t.Fatalf("a second publish opens nothing: %d %v", code, out)
	}
}
