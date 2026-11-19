package store

// The store describes itself (D-203). The laws:
//   - the description is created once and never written over;
//   - a schema whose bytes are not its hash is refused, and one in the
//     store is a break;
//   - a field marked content-hash names a blob, through $ref, anyOf,
//     items, and additionalProperties, and an unmarked one names none;
//   - verify follows the marks from each blob and from each log record,
//     and a missing blob is a break that names what refers to it;
//   - gc keeps every blob a mark names, from a blob or a log record, and
//     still collects an orphan.

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"testing"
	"time"
)

// A pick-like kind: a set whose items name a network by a marked field,
// and a note that is not marked.
const testSetSchema = `{"$defs":{"Pick":{"type":"object","properties":{"id":{"type":"string"},"network_hash":{"type":"string","format":"content-hash"},"note":{"type":"string"},"also":{"anyOf":[{"type":"null"},{"type":"string","format":"content-hash"}]}}}},"type":"object","properties":{"kind":{"const":"set"},"picks":{"type":"array","items":{"$ref":"#/$defs/Pick"}},"by_hash":{"type":"object","additionalProperties":{"$ref":"#/$defs/Pick"}}}}`

func testTable() []byte {
	return []byte(`{"kind":"kind_table","schema_version":1,"kinds":[{"store_kind":"set","tag":"set","versions":[{"version":1,"schema":"` + hashOf([]byte(testSetSchema)) + `"}],"records":[{"kind":"pick","collection":"picks"}]}]}`)
}

func describe(t *testing.T, s *Store) {
	t.Helper()
	if err := s.Describe(testTable(), map[string][]byte{hashOf([]byte(testSetSchema)): []byte(testSetSchema)}); err != nil {
		t.Fatal(err)
	}
}

func TestTheDescriptionIsCreatedOnceAndNeverWrittenOver(t *testing.T) {
	eachBackend(t, func(t *testing.T, b Backend) {
		ctx := context.Background()
		if err := b.Create(ctx, ReadmeKey, []byte("an older README"), nil); err != nil {
			t.Fatal(err)
		}
		s := New(b)
		describe(t, s)
		describe(t, s)
		layout, _, err := b.Get(ctx, LayoutKey)
		if err != nil || !strings.Contains(string(layout), `"hash":"sha256"`) {
			t.Fatalf("the layout: %s %v", layout, err)
		}
		if readme, _, _ := b.Get(ctx, ReadmeKey); string(readme) != "an older README" {
			t.Fatalf("an object that exists is never written over: %q", readme)
		}
		if _, _, err := b.Get(ctx, schemaKey("set", 1, hashOf([]byte(testSetSchema)))); err != nil {
			t.Fatalf("the schema, by its hash: %v", err)
		}
		if err := s.Describe(testTable(), map[string][]byte{hashOf([]byte(testSetSchema)): []byte(`{}`)}); err == nil {
			t.Fatal("a schema whose bytes are not its hash is refused")
		}
	})
}

func TestAMarkNamesABlobAndNothingElseDoes(t *testing.T) {
	b := NewMemory()
	describe(t, New(b))
	d, err := readDescribed(context.Background(), b)
	if err != nil {
		t.Fatal(err)
	}
	h := func(c string) string { return "sha256:" + strings.Repeat(c, 64) }
	doc := `{"kind":"set","picks":[{"id":"a","network_hash":"` + h("a") + `","note":"` + h("b") + `","also":"` + h("c") + `"},{"id":"b","also":null}],"by_hash":{"x":{"network_hash":"` + h("d") + `"}},"other":"` + h("e") + `"}`
	got := d.refs("set", 1, []byte(doc))
	want := []string{strings.Repeat("a", 64), strings.Repeat("c", 64), strings.Repeat("d", 64)}
	if strings.Join(got, ",") != strings.Join(want, ",") {
		t.Fatalf("the marked fields: %v, want %v", got, want)
	}
	if !d.marks("set", 1) || d.marks("set", 2) || d.marks("network_document", 2) {
		t.Fatal("a kind marks a field only when one of its schemas does")
	}
	if got := d.recordRefs("pick", 1, json.RawMessage(`{"id":"a","network_hash":"`+h("f")+`"}`)); len(got) != 1 || got[0] != strings.Repeat("f", 64) {
		t.Fatalf("a log record is walked by its collection's item: %v", got)
	}
}

// pickEntry is a log_entry with one change of a pick record.
func pickEntry(log string, seq int, after string) []byte {
	return []byte(`{"kind":"log_entry","schema_version":1,"log":"` + log + `","seq":` + itoa(seq) + `,"writer":"w","at_ms":1,"module":"m","intent":{"op":"t"},"changes":[{"kind":"pick","version":1,"id":"a","after":` + after + `}]}`)
}

func itoa(n int) string { b, _ := json.Marshal(n); return string(b) }

func TestVerifyFollowsTheMarks(t *testing.T) {
	eachBackend(t, func(t *testing.T, b Backend) {
		s := New(b)
		describe(t, s)
		network, _ := s.PutBlob([]byte(`{"kind":"network"}`), env("network_document"), "")
		missing := "sha256:" + strings.Repeat("9", 64)
		set := `{"kind":"set","picks":[{"id":"a","network_hash":"` + network + `"}]}`
		if _, _, err := s.CAS("set", "", []byte(set), Envelope{Kind: "set", SchemaVersion: 1, ProducedBy: "x"}, "", "w", 1); err != nil {
			t.Fatal(err)
		}
		if err := s.AppendEntry("plans", 1, pickEntry("plans", 1, `{"id":"a","network_hash":"`+network+`"}`)); err != nil {
			t.Fatal(err)
		}
		r, err := Verify(context.Background(), b)
		if err != nil || r.Referenced != 1 || r.Schemas != 1 || r.Tables != 1 {
			t.Fatalf("a store whose marks name stored blobs verifies: %+v %v", r, err)
		}

		if err := s.AppendEntry("plans", 2, pickEntry("plans", 2, `{"id":"a","network_hash":"`+missing+`"}`)); err != nil {
			t.Fatal(err)
		}
		_, err = Verify(context.Background(), b)
		var ve *VerifyError
		if !errors.As(err, &ve) || !strings.Contains(ve.Reason, "logs/plans/entries/") {
			t.Fatalf("a log record naming a missing blob is a break naming the entry: %v", err)
		}
	})
}

func TestVerifyNamesTheBlobThatRefersToAMissingOne(t *testing.T) {
	b := NewMemory()
	s := New(b)
	describe(t, s)
	set := `{"kind":"set","picks":[{"id":"a","network_hash":"sha256:` + strings.Repeat("8", 64) + `"}]}`
	hash, _, _ := s.CAS("set", "", []byte(set), Envelope{Kind: "set", SchemaVersion: 1, ProducedBy: "x"}, "", "w", 1)
	_, err := Verify(context.Background(), b)
	var ve *VerifyError
	if !errors.As(err, &ve) || !strings.Contains(ve.Reason, strings.TrimPrefix(hash, "sha256:")) {
		t.Fatalf("the break names the blob that refers: %v", err)
	}
	// A schema in the store that is not its hash is a break.
	b2 := NewMemory()
	b2.Create(context.Background(), schemaKey("set", 1, "sha256:"+strings.Repeat("1", 64)), []byte(`{}`), nil)
	if _, err := Verify(context.Background(), b2); !errors.As(err, &ve) || !strings.Contains(ve.Reason, "hash to") {
		t.Fatalf("a schema that is not its hash: %v", err)
	}
}

func TestGCKeepsWhatAMarkNames(t *testing.T) {
	eachBackend(t, func(t *testing.T, b Backend) {
		s := New(b)
		describe(t, s)
		byBlob, _ := s.PutBlob([]byte(`{"kind":"network","n":1}`), env("network_document"), "")
		byLog, _ := s.PutBlob([]byte(`{"kind":"network","n":2}`), env("network_document"), "")
		orphan, _ := s.PutBlob([]byte(`{"kind":"network","n":3}`), env("network_document"), "")
		set := `{"kind":"set","picks":[{"id":"a","network_hash":"` + byBlob + `"}]}`
		if _, _, err := s.CAS("set", "", []byte(set), Envelope{Kind: "set", SchemaVersion: 1, ProducedBy: "x"}, "", "w", 1); err != nil {
			t.Fatal(err)
		}
		if err := s.AppendEntry("plans", 1, pickEntry("plans", 1, `{"id":"a","network_hash":"`+byLog+`"}`)); err != nil {
			t.Fatal(err)
		}
		s.now = func() time.Time { return time.Now().Add(OrphanGrace + time.Hour) }
		r, err := s.GC(OrphanGraceMs)
		if err != nil || r.Removed != 1 {
			t.Fatalf("gc: %+v %v", r, err)
		}
		for what, h := range map[string]string{"a blob's mark": byBlob, "a log record's mark": byLog} {
			if _, _, err := s.GetBlob(h); err != nil {
				t.Fatalf("%s keeps %s: %v", what, h, err)
			}
		}
		s.cache.drop(orphan)
		if _, _, err := New(b).GetBlob(orphan); !errors.Is(err, ErrNotFound) {
			t.Fatalf("an orphan is still collected: %v", err)
		}
	})
}
