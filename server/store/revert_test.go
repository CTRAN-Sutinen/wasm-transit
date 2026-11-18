package store

// The emergency procedure (docs/FORMATS.md, "Rolling a version back"):
// a pointer moved back to a head from before an upgrade. The move is a
// new logged commit of that head's document, so the log keeps every
// commit; the tool names the commits the move dropped, each with its
// writer and its document, which stays readable under its hash.

import (
	"strings"
	"testing"
)

func TestRevertMovesAPointerBackAndNamesWhatItDropped(t *testing.T) {
	s := New(NewMemory())
	v2 := Envelope{Kind: "edit_records", SchemaVersion: 2, ProducedBy: "cap-sync@sha256:a"}
	v3 := Envelope{Kind: "edit_records", SchemaVersion: 3, ProducedBy: "cap-sync@sha256:b"}
	before, _, err := s.CAS("edits", "", []byte(`{"v":2}`), v2, "", "alice", 1)
	if err != nil {
		t.Fatal(err)
	}
	upgraded, _, _ := s.CAS("edits", before, []byte(`{"v":3}`), v3, "", "migrate:sha256:m", 2)
	later, _, _ := s.CAS("edits", upgraded, []byte(`{"v":3,"more":1}`), v3, "", "bob", 3)

	report, err := Revert(s, "edits", before, "revert:operator", 4)
	if err != nil {
		t.Fatal(err)
	}
	head, _, _ := s.Head("edits")
	if head.Hash != before || head.Envelope != v2 {
		t.Fatalf("the head is the older document, in its own envelope: %+v", head)
	}
	if report.From != later || report.To != before || len(report.Dropped) != 2 {
		t.Fatalf("the report names the move and the two dropped commits: %+v", report)
	}
	if report.Dropped[0].To != upgraded || report.Dropped[1].To != later || report.Dropped[1].Writer != "bob" {
		t.Fatalf("each dropped commit names its document and writer: %+v", report.Dropped)
	}
	for _, d := range report.Dropped {
		if _, _, err := s.GetBlob(d.To); err != nil {
			t.Fatalf("a dropped document stays readable under its hash: %v", err)
		}
	}
	entries, _, _ := s.Log("edits", "")
	if last := entries[len(entries)-1]; len(entries) != 4 || last.From != later || last.Writer != "revert:operator" {
		t.Fatalf("the move is a new logged commit: %+v", entries)
	}

	// Already there: nothing moves.
	again, err := Revert(s, "edits", before, "revert:operator", 5)
	if err != nil || again.From != before || len(again.Dropped) != 0 {
		t.Fatalf("a pointer at the head asked for stays: %+v %v", again, err)
	}
	if after, _, _ := s.Log("edits", ""); len(after) != 4 {
		t.Fatalf("no commit when nothing moves: %d entries", len(after))
	}
}

func TestRevertRefusesAHeadThePointerNeverHad(t *testing.T) {
	s := New(NewMemory())
	if _, _, err := s.CAS("edits", "", []byte(`{"v":2}`), Envelope{Kind: "edit_records", SchemaVersion: 2}, "", "alice", 1); err != nil {
		t.Fatal(err)
	}
	if _, err := Revert(s, "edits", "sha256:"+strings.Repeat("0", 64), "revert:operator", 2); err == nil || !strings.Contains(err.Error(), "never") {
		t.Fatalf("a hash that was never the head is refused: %v", err)
	}
	if _, err := Revert(s, "missing", "sha256:"+strings.Repeat("0", 64), "revert:operator", 2); err == nil {
		t.Fatal("a pointer that does not exist is refused")
	}
}
