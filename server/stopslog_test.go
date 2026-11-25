package main

// The stops log over HTTP (D-190, D-197, D-198; logkinds.go, cap-stops
// receive). The properties a writer relies on:
//
//  1. Two writers who change different stops both land. A writer who
//     corrects an event another writer changed since gets a 409 naming
//     the event.
//  2. Completing a request and adding its stop's event is one entry.
//  3. A write sent again with its key lands once.
//  4. A refused op names itself, and nothing is written.
//  5. An empty log's first entry comes from the old pointers' heads.
//  6. A revert puts the stops back and takes out what the entry created.

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"rustwasm/server/store"
)

func newStopsLogServer(t *testing.T, b store.Backend) (*server, string) {
	t.Helper()
	s := newStoreServer(t, b)
	closeModules(t, s)
	s.seedKept(nil)
	mux := storeMux(s)
	mux.HandleFunc("/stops", s.handleKeptRoute)
	mux.HandleFunc("/stop-requests", s.handleKeptRoute)
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	return s, srv.URL
}

func opsPost(t *testing.T, base, key string, ops ...map[string]any) (int, map[string]any) {
	t.Helper()
	body, _ := json.Marshal(map[string]any{"key": key, "ops": ops})
	code, _, resp := httpPost(t, base+"/logs/stops", string(body))
	var out map[string]any
	json.Unmarshal([]byte(resp), &out)
	if out == nil {
		out = map[string]any{"raw": resp}
	}
	return code, out
}

func addEvent(stop string, event map[string]any) map[string]any {
	return map[string]any{"cap": "stops", "op": "add_event", "args": map[string]any{"stop_id": stop, "event": event}}
}

func stopsDocNow(t *testing.T, s *server) (string, map[string][]map[string]any) {
	t.Helper()
	hash, doc, found, err := s.keptHead("stops")
	if err != nil || !found {
		t.Fatalf("the stops document: %v %v", found, err)
	}
	var out struct {
		Stops map[string][]map[string]any `json:"stops"`
	}
	json.Unmarshal(doc, &out)
	return hash, out.Stops
}

func TestTwoWritersOfDifferentStopsBothLandAndOneEventConflicts(t *testing.T) {
	s, base := newStopsLogServer(t, store.NewMemory())
	if code, out := postStops(t, base, "", seededStops(t)); code != 200 {
		t.Fatalf("the stops: %d %v", code, out)
	}
	if code, out := opsPost(t, base, "tablet-1/1", addEvent("A", map[string]any{"on": "2026-09-01", "event": "changed", "note": "bench fixed"})); code != 200 {
		t.Fatalf("writer 1: %d %v", code, out)
	}
	if code, out := opsPost(t, base, "tablet-2/1", addEvent("B", map[string]any{"on": "2026-09-02", "event": "changed", "note": "sign moved"})); code != 200 {
		t.Fatalf("writer 2, another stop: %d %v", code, out)
	}
	_, stops := stopsDocNow(t, s)
	if stops["A"][len(stops["A"])-1]["note"] != "bench fixed" || stops["B"][len(stops["B"])-1]["note"] != "sign moved" {
		t.Fatalf("both landed: %v %v", stops["A"], stops["B"])
	}

	event := stops["A"][len(stops["A"])-1]
	saw, err := canonicalHashOf(event)
	if err != nil {
		t.Fatal(err)
	}
	correct := func(note string) map[string]any {
		return map[string]any{"cap": "stops", "op": "correct_event", "args": map[string]any{"stop_id": "A", "event_id": event["id"], "saw": saw,
			"event": map[string]any{"on": "2026-09-01", "event": "changed", "note": note}}}
	}
	if code, out := opsPost(t, base, "tablet-1/2", correct("bench replaced")); code != 200 {
		t.Fatalf("the first correction: %d %v", code, out)
	}
	code, out := opsPost(t, base, "tablet-2/2", correct("bench painted"))
	if code != http.StatusConflict || out["conflict"] != event["id"] || out["op"] != float64(0) {
		t.Fatalf("the second correction names the event: %d %v", code, out)
	}
}

func TestARequestCompletedAndItsEventAddedAreOneEntry(t *testing.T) {
	s, base := newStopsLogServer(t, store.NewMemory())
	postStops(t, base, "", seededStops(t))
	ordered := map[string]any{"stop": "B", "job": "repair_improve", "status": "ordered", "requested_on": "2026-08-01",
		"work_order": map[string]any{"id": "WO-1", "status": "open"}}
	if code, out := opsPost(t, base, "desk/1", map[string]any{"cap": "requests", "op": "put", "args": map[string]any{"id": "R-1", "request": ordered}}); code != 200 {
		t.Fatalf("the request: %d %v", code, out)
	}
	done := map[string]any{}
	for k, v := range ordered {
		done[k] = v
	}
	done["status"], done["completed_on"] = "completed", "2026-09-10"
	code, out := opsPost(t, base, "crew/7",
		map[string]any{"cap": "requests", "op": "put", "args": map[string]any{"id": "R-1", "request": done}},
		addEvent("B", map[string]any{"on": "2026-09-10", "event": "changed", "note": "pad replaced", "request": "R-1"}))
	if code != 200 {
		t.Fatalf("completed with its event: %d %v", code, out)
	}
	seq := int64(out["seq"].(float64))
	entries, _ := s.store.Entries("stops", seq-1, seq)
	var entry struct {
		Writer  string               `json:"writer"`
		Intent  struct{ Key string } `json:"intent"`
		Changes []struct {
			Kind string `json:"kind"`
			ID   string `json:"id"`
		} `json:"changes"`
	}
	json.Unmarshal([]byte(entries[0].Text), &entry)
	kinds := map[string]bool{}
	for _, c := range entry.Changes {
		kinds[c.Kind+" "+c.ID] = true
	}
	if !kinds["stop B"] || !kinds["stop_request R-1"] || len(entry.Changes) != 2 || entry.Intent.Key != "crew/7" {
		t.Fatalf("one entry, both kinds: %+v", entry)
	}
	if out["stop_requests_hash"] == "" || out["stops_hash"] == "" {
		t.Fatalf("the answer names both documents: %v", out)
	}
}

func TestAKeyLandsOnceAndARefusalNamesItsOp(t *testing.T) {
	s, base := newStopsLogServer(t, store.NewMemory())
	postStops(t, base, "", seededStops(t))
	op := addEvent("C", map[string]any{"on": "2026-09-03", "event": "changed", "note": "shelter glass"})
	_, first := opsPost(t, base, "phone-9/41", op)
	code, again := opsPost(t, base, "phone-9/41", op)
	if code != 200 || again["landed"] != true || again["seq"] != first["seq"] {
		t.Fatalf("the outbox's resend lands once: %d %v (first %v)", code, again, first)
	}
	head, _ := s.store.LogHeadOf("stops")
	if float64(head.Seq) != first["seq"] {
		t.Fatalf("no second entry: head %d, first %v", head.Seq, first["seq"])
	}

	before := head
	code, out := opsPost(t, base, "phone-9/42", op, map[string]any{"cap": "stops", "op": "correct_event",
		"args": map[string]any{"stop_id": "C", "event_id": "e-none", "event": map[string]any{"event": "changed"}}})
	if code != http.StatusBadRequest || out["op"] != float64(1) || !strings.Contains(out["error"].(string), "e-none") {
		t.Fatalf("the second op is named: %d %v", code, out)
	}
	// A resend whose op would no longer pass over the head it changed still
	// lands once.
	suspend := addEvent("B", map[string]any{"on": "2026-09-04", "event": "suspended", "until": "2026-09-30", "note": "paving"})
	_, sent := opsPost(t, base, "phone-9/43", suspend)
	if code, resent := opsPost(t, base, "phone-9/43", suspend); code != 200 || resent["landed"] != true || resent["seq"] != sent["seq"] {
		t.Fatalf("a resend lands once before its op runs again: %d %v (sent %v)", code, resent, sent)
	}
	before, _ = s.store.LogHeadOf("stops")
	if code, out := opsPost(t, base, "x", map[string]any{"cap": "stops", "op": "seed"}); code != 400 {
		t.Fatalf("an op the log does not run: %d %v", code, out)
	}
	if head, _ := s.store.LogHeadOf("stops"); head.Seq != before.Seq {
		t.Fatalf("a refused write writes nothing: %d → %d", before.Seq, head.Seq)
	}
}

func TestTheOldPointersSeedTheStopsLog(t *testing.T) {
	bucket := store.NewMemory()
	seed, _ := json.Marshal(seededStops(t))
	inst, err := NewWasmHost(context.Background(), testWasm)
	if err != nil {
		t.Fatal(err)
	}
	host := inst.As("stops")
	var checked struct {
		Document     string `json:"document"`
		DocumentHash string `json:"document_hash"`
	}
	if err := host.Invoke(context.Background(), map[string]any{"op": "validate", "stops": json.RawMessage(seed)}, &checked); err != nil {
		t.Fatal(err)
	}
	inst.Close(context.Background())
	old := store.New(bucket)
	stopsHash, _, err := old.CAS("stops", "", []byte(checked.Document), store.Envelope{Kind: "stops", SchemaVersion: 1, ProducedBy: "cap-stops@old"}, "", "planner", 1)
	if err != nil {
		t.Fatal(err)
	}

	s, _ := newStopsLogServer(t, bucket)
	hash, _, found, err := s.keptHead("stops")
	if err != nil || !found || hash != stopsHash {
		t.Fatalf("the log serves the pointer's document under its hash: %s vs %s (%v)", hash, stopsHash, err)
	}
	if w := lastLogWriter(t, s, "stops"); w != "migrate:stops" {
		t.Fatalf("entry 1's writer: %q", w)
	}
	entries, _ := s.store.Entries("stops", 0, 1)
	if !strings.Contains(entries[0].Text, "stops "+stopsHash) {
		t.Fatalf("entry 1 names the head it copied: %s", entries[0].Text)
	}
	if ptr, _, _ := s.store.Head("stops"); ptr.Seq != 1 {
		t.Fatalf("the pointer stays as it was: %+v", ptr)
	}
	newStopsLogServer(t, bucket)
	if head, _ := s.store.LogHeadOf("stops"); head.Seq != 1 {
		t.Fatalf("a seeded log is not seeded again: %d", head.Seq)
	}
}

func TestARestartServesTheSameStopsAndARevertPutsThemBack(t *testing.T) {
	bucket := store.NewMemory()
	s, base := newStopsLogServer(t, bucket)
	postStops(t, base, "", seededStops(t))
	before, _ := stopsDocNow(t, s)
	_, out := opsPost(t, base, "k1", addEvent("A", map[string]any{"on": "2026-09-05", "event": "changed", "note": "wrong stop"}),
		addEvent("NEW", map[string]any{"on": "2026-10-01", "event": "planned", "lat": 45.5, "lon": -73.5, "name": "New stop"}))
	seq := int64(out["seq"].(float64))
	after, stops := stopsDocNow(t, s)
	if after == before || stops["NEW"] == nil {
		t.Fatalf("the write landed: %v", out)
	}

	s2, base2 := newStopsLogServer(t, bucket)
	if again, _ := stopsDocNow(t, s2); again != after {
		t.Fatalf("a restarted server serves %s, not %s", again, after)
	}
	_, _, body := httpGet(t, base2+"/logs/stops?since=0")
	var log struct {
		Head    int64 `json:"head"`
		Entries []struct {
			Seq   int64           `json:"seq"`
			Entry json.RawMessage `json:"entry"`
		} `json:"entries"`
	}
	if err := json.Unmarshal([]byte(body), &log); err != nil || log.Head != seq || len(log.Entries) != int(seq) {
		t.Fatalf("GET /logs/stops: %s", body)
	}

	code, _, resp := httpPost(t, base2+"/logs/stops/revert", `{"seq":`+itoa(seq)+`}`)
	if code != 200 {
		t.Fatalf("the revert: %d %s", code, resp)
	}
	back, stops := stopsDocNow(t, s2)
	if back != before || stops["NEW"] != nil {
		t.Fatalf("A back, and the stop the entry created out of the document (D-198): %s vs %s", back, before)
	}
	if code, _, resp := httpPost(t, base2+"/logs/stops/revert", `{"seq":`+itoa(seq)+`}`); code != http.StatusConflict {
		t.Fatalf("a second revert of the same entry: %d %s", code, resp)
	}
}

// canonicalHashOf is the canonical hash of a value of strings only: Go
// encodes a map with sorted keys and no space, which is RFC 8785.
func canonicalHashOf(v any) (string, error) {
	raw, err := json.Marshal(v)
	return sha(raw), err
}

func itoa(n int64) string {
	b, _ := json.Marshal(n)
	return string(b)
}

func TestAGapInTheStopsLogStopsWritesUntilItsCauseIsNamed(t *testing.T) {
	bucket := store.NewMemory()
	_, base := newStopsLogServer(t, bucket)
	postStops(t, base, "", seededStops(t))
	opsPost(t, base, "k1", addEvent("A", map[string]any{"on": "2026-09-05", "event": "changed", "note": "one"}))
	opsPost(t, base, "k2", addEvent("B", map[string]any{"on": "2026-09-06", "event": "changed", "note": "two"}))
	bucket.Delete(context.Background(), "logs/stops/entries/00000000000000000002.json")

	_, base2 := newStopsLogServer(t, bucket)
	code, _ := opsPost(t, base2, "k3", addEvent("C", map[string]any{"on": "2026-09-07", "event": "changed", "note": "three"}))
	if code != http.StatusServiceUnavailable {
		t.Fatalf("a write over a gap: %d", code)
	}
	if code, _, resp := httpPost(t, base2+"/logs/stops/close-gap", `{"seq":2,"cause":"deleted by hand"}`); code != 200 {
		t.Fatalf("close the gap: %d %s", code, resp)
	}
	if code, out := opsPost(t, base2, "k3", addEvent("C", map[string]any{"on": "2026-09-07", "event": "changed", "note": "three"})); code != 200 {
		t.Fatalf("writes go on: %d %v", code, out)
	}
}

// An old v1 stop_requests pointer seeds the log at the current version (D-234).
func TestAnOldV1RequestsPointerSeedsTheLogAtTheCurrentVersion(t *testing.T) {
	bucket := store.NewMemory()
	v1 := []byte(`{"kind":"stop_requests","schema_version":1,"requests":{"R-1":{"stop":"6124","job":"repair_improve","status":"submitted","routes":["37"]}}}`)
	if _, _, err := store.New(bucket).CAS("stop_requests", "", v1, store.Envelope{Kind: "stop_requests", SchemaVersion: 1, ProducedBy: "cap-requests@old"}, "", "planner", 1); err != nil {
		t.Fatal(err)
	}
	s, _ := newStopsLogServer(t, bucket)
	_, body, found, err := s.keptHead("stop_requests")
	if err != nil || !found {
		t.Fatalf("the log serves a stop requests head: %v %v", found, err)
	}
	if !strings.Contains(string(body), `"schema_version":2`) || !strings.Contains(string(body), `"routes":["37"]`) {
		t.Fatalf("the head is the v1 document brought forward: %s", body)
	}
}
