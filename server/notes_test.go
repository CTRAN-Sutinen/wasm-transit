package main

// Pins D-214 in store mode: cap-notes writes each note with the verified
// writer and the server's clock, the same text again lands no entry, and
// the manifest pins the head.

import (
	"encoding/json"
	"net/http/httptest"
	"testing"

	"rustwasm/server/store"
)

func TestNotesWritesAreStampedAndLogged(t *testing.T) {
	s := newStoreServer(t, store.NewMemory())
	closeModules(t, s)
	mux := storeMux(s)
	mux.HandleFunc("/notes", s.handleKeptRoute)
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	base := srv.URL

	post := func(id string, target map[string]any, text string) (int, map[string]any) {
		body, _ := json.Marshal(map[string]any{"id": id, "target": target, "text": text})
		code, _, resp := httpPost(t, base+"/notes", string(body))
		var out map[string]any
		json.Unmarshal([]byte(resp), &out)
		return code, out
	}
	trip := map[string]any{"kind": "trip", "trip": "t10", "date": "2026-10-23", "start_secs": 28800}
	if code, out := post("n1", trip, " "); code != 422 {
		t.Fatalf("a blank note is refused: %d %v", code, out)
	}
	code, out := post("n1", trip, "No operator.")
	if code != 200 {
		t.Fatalf("a note: %d %v", code, out)
	}
	first := out["notes_hash"].(string)
	if code, out := post("n1", trip, "No operator."); code != 200 || out["notes_hash"] != first {
		t.Fatalf("the same text again changes nothing: %d %v", code, out)
	}
	if code, out := post("n1", map[string]any{"kind": "stop", "stop": "A"}, "No operator."); code != 422 {
		t.Fatalf("a note's target never changes: %d %v", code, out)
	}
	code, out = post("n1", trip, "No operator. The next trip is covered.")
	if code != 200 || out["notes_hash"] == first {
		t.Fatalf("an edit: %d %v", code, out)
	}
	hash := out["notes_hash"].(string)

	_, _, body := httpGet(t, base+"/manifest")
	var m struct {
		NotesHash string `json:"notes_hash"`
	}
	json.Unmarshal([]byte(body), &m)
	if m.NotesHash != hash {
		t.Fatalf("the manifest pins the head: %q vs %q", m.NotesHash, hash)
	}
	code, hdr, blob := httpGet(t, base+"/blobs/"+hash)
	if code != 200 || hdr.Get("X-Document-Kind") != "notes" || sha([]byte(blob)) != hash {
		t.Fatalf("the notes blob: %d kind=%q", code, hdr.Get("X-Document-Kind"))
	}
	var doc struct {
		Notes map[string]struct {
			Written struct{ By string }  `json:"written"`
			Edited  *struct{ By string } `json:"edited"`
		} `json:"notes"`
	}
	json.Unmarshal([]byte(blob), &doc)
	if n := doc.Notes["n1"]; n.Written.By != "anonymous" || n.Edited == nil || n.Edited.By != "anonymous" {
		t.Fatalf("the server stamps the writer it verified (sign-in off: anonymous): %s", blob)
	}
	code, _, entries := httpGet(t, base+"/logs/notes")
	var log struct {
		Entries []json.RawMessage `json:"entries"`
	}
	json.Unmarshal([]byte(entries), &log)
	if code != 200 || len(log.Entries) != 2 {
		t.Fatalf("the note and its edit are the notes log's entries, and the unchanged write is none: %d %s", code, entries)
	}
}
