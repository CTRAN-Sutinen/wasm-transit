package main

// The stops document in store mode (D-152, D-153, D-157): a write goes
// through the receive path — cap-stops validates it and answers its
// canonical bytes, a stale write is refused — and a pick only reads the
// document, so a stops write changes no pick.

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

// withSuspension is the document with stop A suspended from `on` until
// `until`.
func withSuspension(doc map[string]any, on, until string) map[string]any {
	raw, _ := json.Marshal(doc)
	var out map[string]any
	json.Unmarshal(raw, &out)
	stops := out["stops"].(map[string]any)
	stops["A"] = append(stops["A"].([]any), map[string]any{"on": on, "event": "suspended", "until": until})
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
	bad := map[string]any{"stops": map[string]any{"a": []any{map[string]any{"event": "in_service", "lat": 45, "lon": -122}, map[string]any{"on": "2026-09-01", "event": "in_service"}}}}
	if code, out := postStops(t, base, "", bad); code != 422 || !strings.Contains(out["refusals"].(any).([]any)[0].(map[string]any)["problem"].(map[string]any)["kind"].(string), "already_in_service") {
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
	if code, out := postStops(t, base, "", withSuspension(seed, "2026-10-23", "2026-10-24")); code != 409 {
		t.Fatalf("a write from a version that is no longer the head is refused: %d %v", code, out)
	}
	raw, _ := json.Marshal(map[string]any{"from": hash, "document": "{}"})
	if code, _, _ := httpPost(t, base+"/pointers/stops", string(raw)); code != 400 {
		t.Fatalf("the stops document is written only through POST /stops: %d", code)
	}
}

// A pick only reads the stops document (D-157): a suspension written while
// a pick is published leaves every pick's network as it was.
func TestAStopsWriteChangesNoPick(t *testing.T) {
	s, base := newStopsServer(t, store.NewMemory())
	if code, out := publishPick(t, base, fixtureNetwork(t, "fall-2026", "Fall 2026", "2026-09-07")); code != 200 {
		t.Fatalf("publish fall: %d %v", code, out)
	}
	before := pickHashes(t, s)
	seed := seededStops(t)
	code, out := postStops(t, base, "", seed)
	if code != 200 {
		t.Fatalf("the seed: %d %v", code, out)
	}
	code, out = postStops(t, base, out["stops_hash"].(string), withSuspension(seed, "2026-10-23", "2026-10-24"))
	if code != 200 {
		t.Fatalf("the suspension: %d %v", code, out)
	}
	if _, has := out["republished"]; has {
		t.Fatalf("a stops write republishes nothing: %v", out)
	}
	if got := pickHashes(t, s); got["fall-2026"] != before["fall-2026"] {
		t.Fatalf("the pick is as it was: %v -> %v", before, got)
	}
}
