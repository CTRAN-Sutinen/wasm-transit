package main

// Pins the stops document in store mode.

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

func newStopsServer(t *testing.T, b store.Backend) (*server, string) {
	t.Helper()
	s := newStoreServer(t, b)
	s.plans = newPlansHost(t)
	s.fixedToday = "2026-08-01"
	closeModules(t, s)
	mux := storeMux(s)
	mux.HandleFunc("/plans", s.handlePlans)
	mux.HandleFunc("/plans/retire", s.handleRetire)
	mux.HandleFunc("/stops", s.handleKeptRoute)
	mux.HandleFunc("/feed", s.handleFeed)
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	return s, srv.URL
}

// seededStops runs once per test binary, because each instance costs
// seconds under -race.
var (
	seededOnce sync.Once
	seededRaw  []byte
	seededErr  error
)

func seededStops(t *testing.T) map[string]any {
	t.Helper()
	fixtureNetwork(t, "x", "x", "")
	seededOnce.Do(func() {
		inst, err := NewWasmHost(context.Background(), testWasm)
		if err != nil {
			seededErr = err
			return
		}
		defer inst.Close(context.Background())
		host := inst.As("stops")
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
	blob := plansSet(t, s)
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

// A stops write republishes nothing and changes no pick.
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

// Pins that a published pick reads each stop's record on its first date
// and warns of a visited stop that is not in service.
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
	// The adoption names the stops log entry that it read (D-191).
	stopsHead, _ := s.store.LogHeadOf("stops")
	var set struct {
		Plans []struct {
			StopsSeq int64 `json:"stops_seq"`
		} `json:"plans"`
	}
	json.Unmarshal(plansSet(t, s), &set)
	if len(set.Plans) != 1 || set.Plans[0].StopsSeq != stopsHead.Seq || stopsHead.Seq == 0 {
		t.Fatalf("the pick names stops entry %d: %+v", stopsHead.Seq, set)
	}
}

// Pins that a new survey names the writer, not the name the page sent,
// and a held survey keeps its own.
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
		s.handleKeptRoute(w, req)
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
		var doc map[string]any
		json.Unmarshal(logBlob(t, s, hash), &doc)
		return doc["stops"].(map[string]any)["A"].([]any)
	}
	hash := out["stops_hash"].(string)
	if by := stored(hash)[1].(map[string]any)["surveyed_by"]; by != "Rosa Ortiz" {
		t.Fatalf("the surveyor is the writer: %v", by)
	}
	var held map[string]any
	json.Unmarshal(logBlob(t, s, hash), &held)
	code, out = post(hash, survey(held, "2026-09-15", ""), "Sam Lee")
	if code != 200 {
		t.Fatalf("a second survey: %d %v", code, out)
	}
	events := stored(out["stops_hash"].(string))
	if a, b := events[1].(map[string]any)["surveyed_by"], events[2].(map[string]any)["surveyed_by"]; a != "Rosa Ortiz" || b != "Sam Lee" {
		t.Fatalf("held %v, new %v", a, b)
	}
}

// Pins that GET /feed writes a stop moved inside the pick's period as one
// stops.txt row per version. The version in force on the last
// dates keeps the stop's id, and the earlier one is `<id>@<first date>`.
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

// Pins that a feed that differs from the stops document records an event
// on its first service date, written as `import:feed`, and one that does
// not differ writes nothing.
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
	feeder := fileFeed(sampleFeed, testWasm)
	feed, err := feeder.Load()
	if err != nil {
		t.Fatal(err)
	}
	if err := s.recordFeed(context.Background(), feed); err != nil {
		t.Fatal(err)
	}
	head, held, _, err := s.keptHead("stops")
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
	if w := lastLogWriter(t, s, "stops"); w != "import:feed" {
		t.Fatalf("the writer is the import: %q", w)
	}
	if err := s.recordFeed(context.Background(), feed); err != nil {
		t.Fatal(err)
	}
	if again, _, _, _ := s.keptHead("stops"); again != head {
		t.Fatalf("a feed that differs in nothing writes nothing: %s → %s", head, again)
	}
}

func logBlob(t *testing.T, s *server, hash string) []byte {
	t.Helper()
	_, doc, ok := s.keptDoc(hash)
	if !ok {
		t.Fatalf("the stops log holds no document %s now", hash)
	}
	return doc
}

func lastLogWriter(t *testing.T, s *server, name string) string {
	t.Helper()
	head, err := s.store.LogHeadOf(name)
	if err != nil || head.Seq == 0 {
		t.Fatalf("the %s log: %+v %v", name, head, err)
	}
	entries, err := s.store.Entries(name, head.Seq-1, head.Seq)
	if err != nil || len(entries) != 1 {
		t.Fatalf("entry %d: %v", head.Seq, err)
	}
	var e struct {
		Writer string `json:"writer"`
	}
	json.Unmarshal([]byte(entries[0].Text), &e)
	return e.Writer
}
