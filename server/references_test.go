package main

// The references a write names (D-233; logkinds.go, caplog.go):
//
//  1. A person's write that adds a reference to a record missing from
//     the world its writer read is refused, 422 with `references`;
//     a writer that sends no seq read now.
//  2. A write whose reference held as of the seq its writer read, and
//     is broken now, lands, with the reference as its warning.
//  3. A revert that takes out a record that another names answers the
//     reference it broke.

import (
	"encoding/json"
	"net/http"
	"testing"

	"rustwasm/server/store"
)

// putRequest is the requests op that places request id naming stop.
func putRequest(id, stop string) map[string]any {
	return map[string]any{"cap": "requests", "op": "put", "args": map[string]any{"id": id, "new": true,
		"request": map[string]any{"stop": stop, "job": "new", "status": "drafted", "requested_on": "2026-09-01"}}}
}

// postAt is POST /logs/stops with the seq its writer read.
func postAt(t *testing.T, base, key string, readSeq int64, ops ...map[string]any) (int, map[string]any) {
	t.Helper()
	body, _ := json.Marshal(map[string]any{"key": key, "ops": ops, "read_seq": readSeq})
	code, _, resp := httpPost(t, base+"/logs/stops", string(body))
	var out map[string]any
	json.Unmarshal([]byte(resp), &out)
	if out == nil {
		out = map[string]any{"raw": resp}
	}
	return code, out
}

// refs is the answer's references, each as "<id> names <names>".
func refs(out map[string]any) []string {
	var named []string
	list, _ := out["references"].([]any)
	for _, r := range list {
		m := r.(map[string]any)
		named = append(named, m["id"].(string)+" names "+m["names"].(string))
	}
	return named
}

func TestAWriteIsJudgedAsOfTheSeqItsWriterRead(t *testing.T) {
	s, base := newStopsLogServer(t, store.NewMemory())
	if code, out := postStops(t, base, "", seededStops(t)); code != 200 {
		t.Fatalf("the stops: %d %v", code, out)
	}

	// 1. A request naming a stop no log holds: refused, nothing written.
	before := s.keptSeq("stops")
	code, out := opsPost(t, base, "desk/1", putRequest("R-1", "NO-SUCH-STOP"))
	if code != http.StatusUnprocessableEntity || len(refs(out)) != 1 || refs(out)[0] != "R-1 names NO-SUCH-STOP" {
		t.Fatalf("a reference the writer could not have seen is refused: %d %v", code, out)
	}
	if s.keptSeq("stops") != before {
		t.Fatalf("a refused write writes nothing")
	}

	// A new stop N, read by a planner at its seq; then its entry reverted.
	if code, out := opsPost(t, base, "desk/2", map[string]any{"cap": "stops", "op": "add_event", "args": map[string]any{"stop_id": "N", "new": true,
		"event": map[string]any{"on": "2026-09-01", "event": "planned", "lat": 45.6, "lon": -122.6}}}); code != 200 {
		t.Fatalf("the new stop: %d %v", code, out)
	}
	read := s.keptSeq("stops")
	code, _, resp := httpPost(t, base+"/logs/stops/revert", `{"seq":`+itoa(read)+`}`)
	if code != 200 {
		t.Fatalf("the revert: %d %s", code, resp)
	}

	// 2. The planner's request, as of the seq they read: N held then, so
	// it lands, with N named as its warning.
	code, out = postAt(t, base, "planner/1", read, putRequest("R-2", "N"))
	if code != 200 || len(refs(out)) != 1 || refs(out)[0] != "R-2 names N" {
		t.Fatalf("a write that held as of its read lands with its warning: %d %v", code, out)
	}
	// The same write read now is refused.
	if code, out := opsPost(t, base, "desk/3", putRequest("R-3", "N")); code != http.StatusUnprocessableEntity {
		t.Fatalf("read now, N is gone: %d %v", code, out)
	}

	// 3. A request naming stop A, then the revert of A's own first entry
	// answers the reference it breaks.
	if code, out := opsPost(t, base, "desk/4", map[string]any{"cap": "stops", "op": "add_event", "args": map[string]any{"stop_id": "M", "new": true,
		"event": map[string]any{"on": "2026-09-01", "event": "planned", "lat": 45.61, "lon": -122.61}}}); code != 200 {
		t.Fatalf("stop M: %d %v", code, out)
	}
	madeM := s.keptSeq("stops")
	if code, out := opsPost(t, base, "desk/5", putRequest("R-4", "M")); code != 200 || out["references"] != nil {
		t.Fatalf("a request naming a held stop has no warning: %d %v", code, out)
	}
	code, _, resp = httpPost(t, base+"/logs/stops/revert", `{"seq":`+itoa(madeM)+`}`)
	var reverted map[string]any
	json.Unmarshal([]byte(resp), &reverted)
	if code != 200 || len(refs(reverted)) != 1 || refs(reverted)[0] != "R-4 names M" {
		t.Fatalf("the revert answers the reference it breaks: %d %s", code, resp)
	}
}

// The stop rule of a request (D-238): a planner who retires a stop that
// an open request names is warned with the request; a request then
// written on that stop, read now, is refused with the rule's reason.
func TestRetiringAStopWarnsOfTheOpenRequestThatNamesIt(t *testing.T) {
	s, base := newStopsLogServer(t, store.NewMemory())
	s.fixedToday = "2026-11-21"
	if code, out := postStops(t, base, "", seededStops(t)); code != 200 {
		t.Fatalf("the stops: %d %v", code, out)
	}
	if code, out := opsPost(t, base, "desk/1", map[string]any{"cap": "stops", "op": "add_event", "args": map[string]any{"stop_id": "N", "new": true,
		"event": map[string]any{"on": "2026-01-01", "event": "in_service", "lat": 45.6, "lon": -122.6}}}); code != 200 {
		t.Fatalf("stop N: %d %v", code, out)
	}
	if code, out := opsPost(t, base, "desk/2", putRequest("R-1", "N")); code != 200 {
		t.Fatalf("an open request on N: %d %v", code, out)
	}
	code, out := opsPost(t, base, "planner/1", map[string]any{"cap": "stops", "op": "add_event", "args": map[string]any{"stop_id": "N",
		"event": map[string]any{"on": "2026-11-01", "event": "retired"}}})
	rule, _ := firstRule(out)
	if code != 200 || len(refs(out)) != 1 || refs(out)[0] != "R-1 names N" || rule != "was retired on 2026-11-01" {
		t.Fatalf("the retirement lands with the open request as its warning: %d %v", code, out)
	}
	code, out = opsPost(t, base, "desk/3", putRequest("R-2", "N"))
	if rule, ok := firstRule(out); code != http.StatusUnprocessableEntity || !ok || rule != "was retired on 2026-11-01" {
		t.Fatalf("a new open request on a stop retired when read is refused: %d %v", code, out)
	}
}

// firstRule is the first reference's rule, when it has one.
func firstRule(out map[string]any) (string, bool) {
	list, _ := out["references"].([]any)
	if len(list) == 0 {
		return "", false
	}
	rule, ok := list[0].(map[string]any)["rule"].(string)
	return rule, ok
}
