package main

// The intent write over HTTP (D-189, D-222; logwrite.go): the laws an
// outbox relies on.
//
//  1. A write whose answer is lost and that is resent with its key
//     lands once, even after another writer changed the same record.
//  2. A write to one record lands after another writer changed a
//     different record of the same document; a write to the record
//     another writer changed since the page read it is a 409 naming it.
//  3. The device's time is kept in the entry as a claim beside the
//     server's stamp, and the server stamps a note's writer and time.
//  4. The manifest names the seq of each log's fold it pins.

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"rustwasm/server/store"
)

// newIntentServer is a store-mode server with the alerts and notes
// modules and the logs' routes.
func newIntentServer(t *testing.T) (*server, string) {
	t.Helper()
	s := newStoreServer(t, store.NewMemory())
	s.alertsWasm, s.notesWasm = alertsWasm, notesWasm
	t.Cleanup(func() {
		for _, h := range []*WasmHost{s.alertsHeld, s.notesHeld} {
			if h != nil {
				h.Close(context.Background())
			}
		}
	})
	srv := httptest.NewServer(storeMux(s))
	t.Cleanup(srv.Close)
	return s, srv.URL
}

// intentPost is POST /logs/<log> with a key, the ops, and the fields
// of the body beside them.
func intentPost(t *testing.T, base, log string, body map[string]any) (int, map[string]any) {
	t.Helper()
	raw, _ := json.Marshal(body)
	code, _, resp := httpPost(t, base+"/logs/"+log, string(raw))
	var out map[string]any
	json.Unmarshal([]byte(resp), &out)
	if out == nil {
		out = map[string]any{"raw": resp}
	}
	return code, out
}

func putAlert(id, header string) map[string]any {
	return map[string]any{"cap": "alerts", "op": "put", "args": map[string]any{"id": id, "alert": map[string]any{
		"status": "published", "effect": "detour", "header": []any{map[string]any{"text": header}},
		"informed": []any{map[string]any{"route": "10"}},
	}}}
}

func writeNote(id, text string) map[string]any {
	return map[string]any{"cap": "notes", "op": "write", "args": map[string]any{"id": id, "text": text,
		"target": map[string]any{"kind": "stop", "stop": "A"}}}
}

// entryAt is entry `seq` of a log, as stored.
func entryAt(t *testing.T, s *server, log string, seq int64) map[string]any {
	t.Helper()
	entries, err := s.store.Entries(log, seq-1, seq)
	if err != nil || len(entries) != 1 {
		t.Fatalf("entry %d of %s: %v %v", seq, log, entries, err)
	}
	var e map[string]any
	json.Unmarshal([]byte(entries[0].Text), &e)
	return e
}

func TestAResentWriteLandsOnceAfterAnotherWriterChangedTheRecord(t *testing.T) {
	s, base := newIntentServer(t)
	code, first := intentPost(t, base, "notes", map[string]any{"key": "tablet-1/1", "ops": []any{writeNote("n1", "No operator.")}})
	if code != 200 {
		t.Fatalf("a note: %d %v", code, first)
	}
	// Another writer edits the note before the lost answer is resent.
	if code, out := intentPost(t, base, "notes", map[string]any{"key": "desk/1", "ops": []any{writeNote("n1", "Covered by run 12.")}}); code != 200 {
		t.Fatalf("the edit: %d %v", code, out)
	}
	code, again := intentPost(t, base, "notes", map[string]any{"key": "tablet-1/1", "ops": []any{writeNote("n1", "No operator.")}})
	if code != 200 || again["landed"] != true || again["seq"] != first["seq"] {
		t.Fatalf("the resend lands once, in its first entry: %d %v (first %v)", code, again, first)
	}
	head, _ := s.store.LogHeadOf(notesLogName)
	if head.Seq != 2 {
		t.Fatalf("no third entry: head %d", head.Seq)
	}
	_, doc, _, _ := s.notesHead()
	var notes struct {
		Notes map[string]struct {
			Text    string `json:"text"`
			Written struct {
				By string `json:"by"`
			} `json:"written"`
		} `json:"notes"`
	}
	json.Unmarshal(doc, &notes)
	if n := notes.Notes["n1"]; n.Text != "Covered by run 12." || n.Written.By != "anonymous" {
		t.Fatalf("the edit stands, and the server named the writer: %+v", n)
	}
}

func TestAWriteLandsOverAnotherRecordAndConflictsOnItsOwn(t *testing.T) {
	s, base := newIntentServer(t)
	if code, out := intentPost(t, base, "alerts", map[string]any{"key": "desk/1", "ops": []any{putAlert("a1", "Route 10 detour"), putAlert("a2", "Route 4 detour")}}); code != 200 {
		t.Fatalf("two alerts: %d %v", code, out)
	}
	_, _, body := httpGet(t, base+"/manifest")
	var m struct {
		ServiceAlertsHash string           `json:"service_alerts_hash"`
		LogSeqs           map[string]int64 `json:"log_seqs"`
	}
	json.Unmarshal([]byte(body), &m)
	read := m.LogSeqs["alerts"]
	if read != 1 || m.ServiceAlertsHash == "" {
		t.Fatalf("the manifest names the alerts log's seq beside its documents: %s", body)
	}
	// Another writer changes a2 after this page read the log.
	if code, out := intentPost(t, base, "alerts", map[string]any{"key": "desk/2", "read_seq": read, "ops": []any{putAlert("a2", "Route 4 detour, Main St")}}); code != 200 {
		t.Fatalf("the other writer: %d %v", code, out)
	}
	// This page changes a1: a different record of the same document lands.
	code, out := intentPost(t, base, "alerts", map[string]any{"key": "tablet-1/1", "read_seq": read, "claimed_at_ms": 1770000000123, "ops": []any{putAlert("a1", "Route 10 detour, 5th Ave")}})
	if code != 200 {
		t.Fatalf("a write to another record lands: %d %v", code, out)
	}
	entry := entryAt(t, s, alertsLogName, int64(out["seq"].(float64)))
	intent := entry["intent"].(map[string]any)
	if intent["claimed_at_ms"] != float64(1770000000123) || entry["at_ms"] == float64(1770000000123) || intent["key"] != "tablet-1/1" {
		t.Fatalf("the device's time is a claim beside the server's stamp: %v", entry)
	}
	// This page changes a2 too, from the same read: a person settles it.
	code, out = intentPost(t, base, "alerts", map[string]any{"key": "tablet-1/2", "read_seq": read, "ops": []any{putAlert("a2", "Route 4 back on its route")}})
	overlap, _ := out["overlap"].(map[string]any)
	if code != http.StatusConflict || overlap["kind"] != "service_alert" || overlap["id"] != "a2" || overlap["seq"] != float64(2) {
		t.Fatalf("the same record is a 409 naming it: %d %v", code, out)
	}
	// Read again, the same write lands.
	if code, out := intentPost(t, base, "alerts", map[string]any{"key": "tablet-1/2", "read_seq": 3, "ops": []any{putAlert("a2", "Route 4 back on its route")}}); code != 200 {
		t.Fatalf("after a read of the head: %d %v", code, out)
	}
	// An op the log does not run, and a refused alert, write nothing.
	if code, out := intentPost(t, base, "alerts", map[string]any{"key": "x", "ops": []any{map[string]any{"cap": "notes", "op": "write"}}}); code != 400 {
		t.Fatalf("another log's op: %d %v", code, out)
	}
	if code, out := intentPost(t, base, "alerts", map[string]any{"key": "y", "ops": []any{putAlert("a3", " ")}}); code != 422 || out["op"] != float64(0) {
		t.Fatalf("a refusal names its op: %d %v", code, out)
	}
	if head, _ := s.store.LogHeadOf(alertsLogName); head.Seq != 4 {
		t.Fatalf("refusals write nothing: head %d", head.Seq)
	}
}

func TestAnAlertEndedThroughItsIntent(t *testing.T) {
	s, base := newIntentServer(t)
	intentPost(t, base, "alerts", map[string]any{"key": "k1", "ops": []any{putAlert("a1", "Route 10 detour")}})
	code, out := intentPost(t, base, "alerts", map[string]any{"key": "k2", "ops": []any{map[string]any{"cap": "alerts", "op": "end", "args": map[string]any{"id": "a1", "at": 1770000000}}}})
	if code != 200 {
		t.Fatalf("end: %d %v", code, out)
	}
	_, doc, _, _ := s.alertsLogHead(func() logDoc { return s.alertsDoc })
	var alerts struct {
		Alerts map[string]map[string]any `json:"alerts"`
	}
	json.Unmarshal(doc, &alerts)
	if a := alerts.Alerts["a1"]; a["status"] != "retired" && a["communication"] == nil {
		t.Fatalf("the alert ended: %v", a)
	}
}
