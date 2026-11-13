package main

// The stops document in store mode (D-152, D-153, D-157): a write goes
// through the receive path — cap-stops validates it and answers its
// canonical bytes, a stale write is refused — and a pick only reads the
// document, so a stops write changes no pick.

import (
	"archive/zip"
	"bytes"
	"context"
	"encoding/csv"
	"encoding/json"
	"io"
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
	mux.HandleFunc("/feed", s.handleFeed)
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

// A pick being published reads the stops document (D-157): each stop's
// record on its first date, and a warning for a stop it visits that is
// not in service over its period.
func TestAPublishedPickReadsTheStops(t *testing.T) {
	s, base := newStopsServer(t, store.NewMemory())
	raw, _ := json.Marshal(seededStops(t))
	var doc map[string]any
	json.Unmarshal(raw, &doc)
	stops := doc["stops"].(map[string]any)
	stops["A"] = append(stops["A"].([]any), map[string]any{"on": "2026-09-01", "event": "changed", "name": "A Street Station"})
	stops["B"] = append(stops["B"].([]any), map[string]any{"on": "2026-10-15", "event": "retired"})
	if code, out := postStops(t, base, "", doc); code != 200 {
		t.Fatalf("the stops: %d %v", code, out)
	}
	code, out := publishPick(t, base, fixtureNetwork(t, "fall-2026", "Fall 2026", "2026-09-07"))
	if code != 200 {
		t.Fatalf("publish fall: %d %v", code, out)
	}
	warned, _ := json.Marshal(out["warnings"])
	if !strings.Contains(string(warned), `"stop_not_in_service":{"from":"2026-10-15","status":"retired","stop":"B"}`) {
		t.Fatalf("the planner is warned of B, retired inside the period: %s", warned)
	}
	_, _, body := httpGet(t, base+"/blobs/"+pickHashes(t, s)["fall-2026"])
	var net struct {
		Stops map[string]struct {
			Name string `json:"name"`
		} `json:"stops"`
	}
	json.Unmarshal([]byte(body), &net)
	if net.Stops["A"].Name != "A Street Station" {
		t.Fatalf("the pick takes A's name from the stops document: %+v", net.Stops["A"])
	}
}

// The surveyor is the writer (D-158): a survey the server does not hold
// names the person writing (here the dev label, X-Writer, auth being
// off), whatever name the page sent; a held survey keeps its own.
func TestASurveyNamesTheWriter(t *testing.T) {
	s, base := newStopsServer(t, store.NewMemory())
	seed := seededStops(t)
	code, out := postStops(t, base, "", seed)
	if code != 200 {
		t.Fatalf("the seed: %d %v", code, out)
	}
	from := out["stops_hash"].(string)
	survey := func(doc map[string]any, on, by string) map[string]any {
		raw, _ := json.Marshal(doc)
		var next map[string]any
		json.Unmarshal(raw, &next)
		stops := next["stops"].(map[string]any)
		stops["A"] = append(stops["A"].([]any), map[string]any{"on": on, "event": "changed", "surveyed_by": by, "site": map[string]any{"surface": "concrete"}})
		return next
	}
	post := func(from string, doc any, writer string) (int, map[string]any) {
		body, _ := json.Marshal(map[string]any{"from": from, "stops": doc})
		req := httptest.NewRequest("POST", "/stops", strings.NewReader(string(body)))
		req.Header.Set("X-Writer", writer)
		w := httptest.NewRecorder()
		s.handleStops(w, req)
		var out map[string]any
		json.Unmarshal(w.Body.Bytes(), &out)
		return w.Code, out
	}
	first := survey(seed, "2026-09-01", "Typed By Hand")
	code, out = post(from, first, "Rosa Ortiz")
	if code != 200 {
		t.Fatalf("a survey: %d %v", code, out)
	}
	stored := func(hash string) []any {
		blob, _, err := s.store.GetBlob(hash)
		if err != nil {
			t.Fatal(err)
		}
		var doc map[string]any
		json.Unmarshal(blob, &doc)
		return doc["stops"].(map[string]any)["A"].([]any)
	}
	hash := out["stops_hash"].(string)
	if by := stored(hash)[1].(map[string]any)["surveyed_by"]; by != "Rosa Ortiz" {
		t.Fatalf("the surveyor is the writer: %v", by)
	}
	// another writer adds a survey: theirs is theirs, the held one stays
	var held map[string]any
	blob, _, _ := s.store.GetBlob(hash)
	json.Unmarshal(blob, &held)
	code, out = post(hash, survey(held, "2026-09-15", ""), "Sam Lee")
	if code != 200 {
		t.Fatalf("a second survey: %d %v", code, out)
	}
	events := stored(out["stops_hash"].(string))
	if a, b := events[1].(map[string]any)["surveyed_by"], events[2].(map[string]any)["surveyed_by"]; a != "Rosa Ortiz" || b != "Sam Lee" {
		t.Fatalf("held %v, new %v", a, b)
	}
}

// GTFS out carries the stops' history (D-151): a stop moved inside the
// pick's period is one stops.txt row per version — the version in force
// on the period's last dates under the stop's own id, the earlier one
// under `<id>@<its first date>` — and GET /feed serves it.
func TestTheFeedCarriesTheStopsHistory(t *testing.T) {
	s, base := newStopsServer(t, store.NewMemory())
	_ = s
	raw, _ := json.Marshal(seededStops(t))
	var doc map[string]any
	json.Unmarshal(raw, &doc)
	stops := doc["stops"].(map[string]any)
	stops["A"] = append(stops["A"].([]any), map[string]any{"on": "2026-10-01", "event": "moved", "lat": 45.501, "lon": -73.501})
	if code, out := postStops(t, base, "", doc); code != 200 {
		t.Fatalf("the stops: %d %v", code, out)
	}
	if code, out := publishPick(t, base, fixtureNetwork(t, "fall-2026", "Fall 2026", "2026-09-07")); code != 200 {
		t.Fatalf("publish fall: %d %v", code, out)
	}
	code, _, body := httpGet(t, base+"/feed?date=2026-09-10")
	if code != 200 {
		t.Fatalf("GET /feed: %d %s", code, body)
	}
	zr, err := zip.NewReader(bytes.NewReader([]byte(body)), int64(len(body)))
	if err != nil {
		t.Fatal(err)
	}
	var stopsTxt string
	for _, f := range zr.File {
		if f.Name == "stops.txt" {
			rc, _ := f.Open()
			b, _ := io.ReadAll(rc)
			rc.Close()
			stopsTxt = string(b)
		}
	}
	rows, err := csv.NewReader(strings.NewReader(stopsTxt)).ReadAll()
	if err != nil || len(rows) == 0 {
		t.Fatalf("stops.txt: %v %q", err, stopsTxt)
	}
	col := map[string]int{}
	for i, h := range rows[0] {
		col[h] = i
	}
	ids := map[string]string{}
	for _, r := range rows[1:] {
		ids[r[col["stop_id"]]] = r[col["stop_lat"]]
	}
	if _, ok := ids["A@2026-09-07"]; !ok {
		t.Fatalf("A's earlier version under its own row: %v", ids)
	}
	if ids["A"] == "" || ids["A"] == ids["A@2026-09-07"] {
		t.Fatalf("A's version in force last, at its new point: %v", ids)
	}
}

// The agency's feed applies, and the planner sees what it did (D-150): a
// feed whose stop A is named otherwise than the stops document records
// the difference as an event on the feed's first service date naming the
// feed, written as `import:feed`; a feed that differs in nothing writes
// nothing.
func TestTheFeedRecordsItsDifferences(t *testing.T) {
	s, base := newStopsServer(t, store.NewMemory())
	raw, _ := json.Marshal(seededStops(t))
	var doc map[string]any
	json.Unmarshal(raw, &doc)
	first := doc["stops"].(map[string]any)["A"].([]any)[0].(map[string]any)
	first["name"] = "Main & First (old sign)"
	if code, out := postStops(t, base, "", doc); code != 200 {
		t.Fatalf("the stops: %d %v", code, out)
	}
	feeder := fileFeed(sampleFeed, networkWasm)
	feed, err := feeder.Load()
	if err != nil {
		t.Fatal(err)
	}
	if err := s.recordFeed(context.Background(), feed); err != nil {
		t.Fatal(err)
	}
	head, held, _, err := s.stopsHead()
	if err != nil {
		t.Fatal(err)
	}
	var after struct {
		Stops map[string][]map[string]any `json:"stops"`
	}
	json.Unmarshal(held, &after)
	last := after.Stops["A"][len(after.Stops["A"])-1]
	if last["event"] != "changed" || last["from_feed"] != feed.hash || last["name"] == nil {
		t.Fatalf("A's difference recorded, naming the feed: %v", last)
	}
	entries, _, _ := s.store.Log(stopsPointer, "")
	if w := entries[len(entries)-1].Writer; w != "import:feed" {
		t.Fatalf("the writer is the import: %q", w)
	}
	// again: nothing differs, nothing is written
	if err := s.recordFeed(context.Background(), feed); err != nil {
		t.Fatal(err)
	}
	if again, _, _, _ := s.stopsHead(); again != head {
		t.Fatalf("a feed that differs in nothing writes nothing: %s → %s", head, again)
	}
}
