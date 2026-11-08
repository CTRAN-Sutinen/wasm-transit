package store

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"testing"
)

// healthy answers a bucket with two pointers, a reused hash, an orphan
// and a leftover trash object — everything verify must accept.
func healthy(t *testing.T, b Backend) (edits, manifest string) {
	t.Helper()
	s := New(b)
	e1, _, err := s.CAS("edits", "", []byte(`[1]`), env("edit_records"), "", "w", 1)
	if err != nil {
		t.Fatal(err)
	}
	e2, _, _ := s.CAS("edits", e1, []byte(`[2]`), env("edit_records"), "", "w", 2)
	m1, _, _ := s.CAS("manifest", "", []byte(`{"m":1}`), env("manifest"), "", "server", 3)
	m2, _, _ := s.CAS("manifest", m1, []byte(`{"m":2}`), env("manifest"), "", "server", 4)
	if _, _, err := s.CAS("manifest", m2, []byte(`{"m":1}`), env("manifest"), "", "server", 5); err != nil {
		t.Fatal(err)
	}
	s.PutBlob([]byte(`["orphan"]`), env("edit_records"), "")
	return e2, m1
}

func TestVerifyPassesAHealthyStore(t *testing.T) {
	eachBackend(t, func(t *testing.T, b Backend) {
		healthy(t, b)
		r, err := Verify(context.Background(), b)
		if err != nil {
			t.Fatalf("a healthy store verifies: %v", err)
		}
		if r.Pointers != 2 || r.Records != 5 || r.Blobs != 4 || r.Orphans != 1 {
			t.Fatalf("report: %+v", r)
		}
	})
}

// Each break is its own fixture: a healthy store, one object changed,
// and verify names that object.
func TestVerifyFailsOnEachBreak(t *testing.T) {
	ctx := context.Background()
	rec := func(seq int64, from, to string) []byte {
		b, _ := json.Marshal(LogEntry{Seq: seq, From: from, To: to, Kind: "edit_records", SchemaVersion: 1, ProducedBy: "x", Writer: "w"})
		return b
	}
	meta := func() map[string]string {
		m, _ := json.Marshal(env("edit_records"))
		return map[string]string{envelopeMeta: string(m)}
	}
	cases := []struct {
		name, key, want string
		breakIt         func(b *Memory, edits, manifest string)
	}{
		{"a gap in the records", "pointers/edits/00000000000000000004.json", "gap",
			func(b *Memory, edits, _ string) {
				b.Create(ctx, recordKey("edits", 4), rec(4, edits, edits), nil)
			}},
		{"a record that does not chain", "pointers/edits/00000000000000000003.json", "chain",
			func(b *Memory, _, manifest string) {
				b.Create(ctx, recordKey("edits", 3), rec(3, manifest, manifest), nil)
			}},
		{"a record whose body names another seq", "pointers/edits/00000000000000000003.json", "seq",
			func(b *Memory, edits, _ string) {
				b.Create(ctx, recordKey("edits", 3), rec(7, edits, edits), nil)
			}},
		{"a record that is not JSON", "pointers/edits/00000000000000000003.json", "record",
			func(b *Memory, _, _ string) {
				b.Create(ctx, recordKey("edits", 3), []byte("{\"seq\":3,\x00\x00"), nil)
			}},
		{"a named blob that is missing", "blobs/", "missing",
			func(b *Memory, edits, _ string) {
				h, _ := ParseHash(edits)
				b.Delete(ctx, blobKey(h))
			}},
		{"a blob whose bytes are not its name", "blobs/", "hash",
			func(b *Memory, edits, _ string) {
				h, _ := ParseHash(edits)
				b.Delete(ctx, blobKey(h))
				b.Create(ctx, blobKey(h), []byte(`[999]`), meta())
			}},
		{"a blob without its envelope", "blobs/", "envelope",
			func(b *Memory, edits, _ string) {
				h, _ := ParseHash(edits)
				b.Delete(ctx, blobKey(h))
				b.Create(ctx, blobKey(h), []byte(`[2]`), nil)
			}},
		{"a key under pointers/ that is no record", "pointers/edits/latest", "not a record",
			func(b *Memory, _, _ string) {
				b.Create(ctx, "pointers/edits/latest", []byte(`{}`), nil)
			}},
		{"a key under blobs/ that is no hash", "blobs/notahash", "not a blob",
			func(b *Memory, _, _ string) {
				b.Create(ctx, "blobs/notahash", []byte(`x`), meta())
			}},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			b := NewMemory()
			edits, manifest := healthy(t, b)
			c.breakIt(b, edits, manifest)
			_, err := Verify(ctx, b)
			var ve *VerifyError
			if !errors.As(err, &ve) {
				t.Fatalf("verify must fail: %v", err)
			}
			if !strings.HasPrefix(ve.Key, c.key) || !strings.Contains(ve.Error(), c.want) {
				t.Fatalf("the failure names %q (%q): %v", c.key, c.want, err)
			}
		})
	}
}

// Verify only reads.
func TestVerifyWritesNothing(t *testing.T) {
	b := NewMemory()
	healthy(t, b)
	before, _ := b.List(context.Background(), "", "")
	if _, err := Verify(context.Background(), readOnly{b}); err != nil {
		t.Fatal(err)
	}
	after, _ := b.List(context.Background(), "", "")
	if len(before) != len(after) {
		t.Fatalf("verify wrote: %d -> %d objects", len(before), len(after))
	}
}

type readOnly struct{ Backend }

func (readOnly) Create(context.Context, string, []byte, map[string]string) error {
	return errors.New("read only")
}
func (readOnly) Delete(context.Context, string) error { return errors.New("read only") }
