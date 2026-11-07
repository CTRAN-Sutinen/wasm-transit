package main

// Document versions at the server's receive boundaries
// (docs/units/document-versioning.md, rule 4):
//
//   - a POST in an older version (an edit document v1, the bare array
//     a client on an older build sends) comes forward through
//     cap-migrate before the merge; the head is always the current
//     version;
//   - a server started on a store holding an older head commits it
//     forward as an ordinary logged CAS (writer migrate:<module hash>),
//     and the older blob stays readable under its hash.

import (
	"encoding/json"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"rustwasm/server/store"
)

const currentEditsPrefix = `{"kind":"edit_records","schema_version":2,`

func TestAnOlderEditDocumentMigratesAtReceive(t *testing.T) {
	s := newStoreServer(t, t.TempDir())
	ts := httptest.NewServer(storeMux(s))
	defer ts.Close()

	code, _, body := httpPost(t, ts.URL+"/edits", editRecords("e1", "accepted", 1, 1000))
	if code != 200 {
		t.Fatalf("an older client's document is accepted: %d %s", code, body)
	}
	if !strings.HasPrefix(body, currentEditsPrefix) {
		t.Fatalf("the answer is the current version: %s", body)
	}
	head, _, _ := s.store.Head("edits")
	if head.Envelope.SchemaVersion != editsVersion {
		t.Fatalf("the head's envelope says v%d, want v%d", head.Envelope.SchemaVersion, editsVersion)
	}
	data, _, _ := s.store.GetBlob(head.Hash)
	if !strings.HasPrefix(string(data), currentEditsPrefix) || !strings.Contains(string(data), `"id":"e1"`) {
		t.Fatalf("the head is the migrated document, its record kept: %s", data)
	}

	// The same through the generic pointer surface.
	req := `{"from":"` + head.Hash + `","client":` + editRecords("e2", "accepted", 1, 2000) + `}`
	if code, _, body := httpPost(t, ts.URL+"/pointers/edits", req); code != 200 {
		t.Fatalf("POST /pointers/edits with a v1 client: %d %s", code, body)
	}
}

func TestANewerEditDocumentIsRefusedNamingBoth(t *testing.T) {
	s := newStoreServer(t, t.TempDir())
	ts := httptest.NewServer(storeMux(s))
	defer ts.Close()
	newer := `{"kind":"edit_records","schema_version":3,"records":[]}`
	code, _, body := httpPost(t, ts.URL+"/edits", newer)
	if code != 400 || !strings.Contains(body, "edit_records v3") || !strings.Contains(body, "reads v2") {
		t.Fatalf("a newer document is refused naming both versions, never rewritten: %d %s", code, body)
	}
}

func TestAnOlderStoreHeadCommitsForwardAtStartup(t *testing.T) {
	dir := t.TempDir()
	// A store an older build wrote: the edits head is a bare array
	// under an envelope that says v1.
	old := []byte(editRecords("e1", "accepted", 3, 1000))
	st, err := store.Open(dir)
	if err != nil {
		t.Fatal(err)
	}
	oldHash, _, err := st.CAS("edits", "", old, store.Envelope{Kind: "edit_records", SchemaVersion: 1, ProducedBy: "cap-sync@sha256:old"}, "", "anonymous", time.Now().UnixMilli())
	if err != nil {
		t.Fatal(err)
	}
	st.Close()

	s := newStoreServer(t, dir)
	s.commitForward()

	head, _, _ := s.store.Head("edits")
	if head.Hash == oldHash || head.Envelope.SchemaVersion != editsVersion {
		t.Fatalf("the head moved to the current version: %+v", head)
	}
	data, _, _ := s.store.GetBlob(head.Hash)
	var doc struct {
		Kind          string            `json:"kind"`
		SchemaVersion int               `json:"schema_version"`
		Records       []json.RawMessage `json:"records"`
	}
	if err := json.Unmarshal(data, &doc); err != nil || doc.Kind != "edit_records" || doc.SchemaVersion != editsVersion || len(doc.Records) != 1 {
		t.Fatalf("the migrated head: %v %s", err, data)
	}
	entries, _, _ := s.store.Log("edits", "")
	last := entries[len(entries)-1]
	m, _ := s.migrator()
	if last.From != oldHash || last.Writer != "migrate:"+m.Hash() {
		t.Fatalf("an ordinary logged CAS from the old head, by the migration module: %+v", last)
	}
	if kept, _, err := s.store.GetBlob(oldHash); err != nil || string(kept) != string(old) {
		t.Fatalf("the older blob stays readable under its hash: %v", err)
	}

	// Already current: a second start commits nothing.
	s.commitForward()
	if after, _, _ := s.store.Log("edits", ""); len(after) != len(entries) {
		t.Fatalf("a current head is not committed again: %d entries, want %d", len(after), len(entries))
	}
}
