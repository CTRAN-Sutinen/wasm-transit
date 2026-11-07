package store

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"sync"
	"testing"
	"time"
)

func env(kind string) Envelope {
	return Envelope{Kind: kind, SchemaVersion: 1, ProducedBy: "cap-sync@sha256:" + strings.Repeat("ab", 32)}
}

// each runs a store law over every backend (contract_test.go): a fresh
// backend, one store over it.
func each(t *testing.T, f func(t *testing.T, b Backend, s *Store)) {
	eachBackend(t, func(t *testing.T, b Backend) { f(t, b, New(b)) })
}

func TestCASCreateMoveAndConflict(t *testing.T) {
	each(t, func(t *testing.T, _ Backend, s *Store) {
		h1, seq, err := s.CAS("edits", "", []byte(`[{"id":"e1"}]`), env("edit_records"), "", "alice", 1000)
		if err != nil || seq != 1 {
			t.Fatalf("create: %v seq=%d", err, seq)
		}
		head, found, err := s.Head("edits")
		if err != nil || !found || head.Hash != h1 || head.Seq != 1 {
			t.Fatalf("head: %+v found=%v err=%v", head, found, err)
		}
		if head.Envelope.Kind != "edit_records" || head.Envelope.SchemaVersion != 1 {
			t.Fatalf("head envelope: %+v", head.Envelope)
		}

		// A second writer that saw h1 commits; one that saw nothing conflicts.
		h2, seq, err := s.CAS("edits", h1, []byte(`[{"id":"e1"},{"id":"e2"}]`), env("edit_records"), "", "bob", 2000)
		if err != nil || seq != 2 {
			t.Fatalf("move: %v seq=%d", err, seq)
		}
		_, _, err = s.CAS("edits", "", []byte(`[]`), env("edit_records"), "", "carol", 3000)
		var ce *ConflictError
		if !errors.As(err, &ce) || ce.Current != h2 {
			t.Fatalf("create-over-existing must conflict with current h2: %v", err)
		}
		_, _, err = s.CAS("edits", h1, []byte(`[]`), env("edit_records"), "", "carol", 3000)
		if !errors.As(err, &ce) || ce.Current != h2 {
			t.Fatalf("stale from must conflict: %v", err)
		}
		// A failed CAS left authority untouched.
		head, _, _ = s.Head("edits")
		if head.Hash != h2 || head.Seq != 2 {
			t.Fatalf("failed CAS moved the pointer: %+v", head)
		}
	})
}

// The at-rest layout (docs/FORMATS.md): a blob is one object at
// blobs/<hex> carrying its envelope as metadata — one write, no
// sidecar — and each commit is one record object at
// pointers/<name>/<seq, 20 digits>.json holding the LogEntry.
func TestACommitIsOneBlobObjectAndOneRecordObject(t *testing.T) {
	each(t, func(t *testing.T, b Backend, s *Store) {
		ctx := context.Background()
		doc := []byte(`[{"id":"e1"}]`)
		h1, _, err := s.CAS("edits", "", doc, env("edit_records"), "", "alice", 1000)
		if err != nil {
			t.Fatal(err)
		}
		hex, _ := ParseHash(h1)
		data, meta, err := b.Get(ctx, "blobs/"+hex)
		if err != nil || string(data) != string(doc) {
			t.Fatalf("blob object: %q %v", data, err)
		}
		var got Envelope
		if err := json.Unmarshal([]byte(meta["envelope"]), &got); err != nil || got != env("edit_records") {
			t.Fatalf("the envelope is the blob's metadata: %q %v", meta, err)
		}
		raw, _, err := b.Get(ctx, "pointers/edits/00000000000000000001.json")
		if err != nil {
			t.Fatalf("record object: %v", err)
		}
		var rec LogEntry
		if err := json.Unmarshal(raw, &rec); err != nil || rec.Seq != 1 || rec.From != "" || rec.To != h1 || rec.Writer != "alice" || rec.AtMs != 1000 {
			t.Fatalf("record body: %s %v", raw, err)
		}
		objs, _ := b.List(ctx, "", "")
		if len(objs) != 2 {
			t.Fatalf("a commit writes exactly two objects: %+v", objs)
		}
	})
}

func TestRedeliveryIsIdempotentAndLogReplays(t *testing.T) {
	each(t, func(t *testing.T, _ Backend, s *Store) {
		doc := []byte(`[{"id":"e1","status":"submitted"}]`)
		h1, _, err := s.CAS("edits", "", doc, env("edit_records"), "", "alice", 1000)
		if err != nil {
			t.Fatal(err)
		}
		// At-least-once redelivery: committing the SAME bytes on top of the
		// same head is a no-op — same hash, same seq, no log growth.
		h1again, seqAgain, err := s.CAS("edits", h1, doc, env("edit_records"), "", "alice", 1001)
		if err != nil || h1again != h1 || seqAgain != 1 {
			t.Fatalf("same-bytes re-commit must be a no-op: %v %s seq=%d", err, h1again, seqAgain)
		}
		head, _, _ := s.Head("edits")
		entries, found, err := s.Log("edits", "")
		if err != nil || !found || len(entries) != 1 {
			t.Fatalf("log: %v found=%v entries=%d", err, found, len(entries))
		}
		if entries[0].From != "" || entries[0].To != head.Hash {
			t.Fatalf("log not chained: %+v", entries)
		}
		// A real change appends; delta since the first hash answers it.
		h2, _, err := s.CAS("edits", h1, []byte(`[{"id":"e1","x":1}]`), env("edit_records"), "", "bob", 2000)
		if err != nil {
			t.Fatal(err)
		}
		delta, found, err := s.Log("edits", entries[0].To)
		if err != nil || !found || len(delta) != 1 || delta[0].To != h2 || delta[0].From != h1 {
			t.Fatalf("delta: %v found=%v %+v", err, found, delta)
		}
		// An unknown since answers not-found (the client falls back to the
		// whole document).
		_, found, err = s.Log("edits", "sha256:"+strings.Repeat("00", 32))
		if err != nil || found {
			t.Fatalf("unknown since must be found=false: %v %v", found, err)
		}
		// A pointer with no commits has no log.
		if _, found, err := s.Log("nothing", ""); err != nil || found {
			t.Fatalf("no pointer, no log: %v %v", found, err)
		}
	})
}

// A delta from a hash the pointer held twice starts after the latest
// time it held it: the client holding those bytes is at least that
// far along, and the entries between are already in its document.
func TestLogSinceARepeatedHashStartsAfterItsLatestRecord(t *testing.T) {
	each(t, func(t *testing.T, _ Backend, s *Store) {
		a, _, _ := s.CAS("manifest", "", []byte(`{"v":"a"}`), env("manifest"), "", "server", 1)
		b, _, _ := s.CAS("manifest", a, []byte(`{"v":"b"}`), env("manifest"), "", "server", 2)
		if _, _, err := s.CAS("manifest", b, []byte(`{"v":"a"}`), env("manifest"), "", "server", 3); err != nil {
			t.Fatal(err)
		}
		c, _, _ := s.CAS("manifest", a, []byte(`{"v":"c"}`), env("manifest"), "", "server", 4)
		delta, found, err := s.Log("manifest", a)
		if err != nil || !found || len(delta) != 1 || delta[0].To != c || delta[0].Seq != 4 {
			t.Fatalf("delta after the latest a: %+v found=%v %v", delta, found, err)
		}
	})
}

func TestCrashBetweenBlobAndRecordLeavesOrphanAndOldAuthority(t *testing.T) {
	each(t, func(t *testing.T, b Backend, s *Store) {
		h1, _, err := s.CAS("edits", "", []byte(`[1]`), env("edit_records"), "", "alice", 1000)
		if err != nil {
			t.Fatal(err)
		}
		// A crash after the blob write but before the record: the blob
		// exists, no record names it.
		orphan, err := s.PutBlob([]byte(`[2]`), env("edit_records"), "")
		if err != nil {
			t.Fatal(err)
		}

		// A fresh process over the same bucket: the old authority is
		// intact, the orphan still there (within grace); GC past the grace
		// collects only the orphan.
		s2 := New(b)
		head, found, _ := s2.Head("edits")
		if !found || head.Hash != h1 || head.Seq != 1 {
			t.Fatalf("crash must leave the old authority readable: %+v", head)
		}
		if removed, err := s2.GC(OrphanGraceMs); err != nil || removed != 0 {
			t.Fatalf("an orphan inside the grace survives: removed=%d %v", removed, err)
		}
		if _, _, err := s2.GetBlob(orphan); err != nil {
			t.Fatalf("orphan blob should still exist within grace: %v", err)
		}
		// Two days on, GC collects the one orphan object.
		s2.now = func() time.Time { return time.Now().Add(48 * time.Hour) }
		removed, err := s2.GC(OrphanGraceMs)
		if err != nil || removed != 1 {
			t.Fatalf("gc: removed=%d err=%v", removed, err)
		}
		if _, _, err := s2.GetBlob(orphan); !errors.Is(err, ErrNotFound) {
			t.Fatalf("orphan must be collected: %v", err)
		}
		if _, _, err := s2.GetBlob(h1); err != nil {
			t.Fatalf("referenced blob must survive GC: %v", err)
		}
	})
}

func TestHashAssertionFailsLoudly(t *testing.T) {
	each(t, func(t *testing.T, _ Backend, s *Store) {
		_, err := s.PutBlob([]byte(`[]`), env("edit_records"), "sha256:"+strings.Repeat("ff", 32))
		if err == nil || !strings.Contains(err.Error(), "hash mismatch") {
			t.Fatalf("a wrong module hash must fail loudly: %v", err)
		}
		_, _, err = s.CAS("edits", "", []byte(`[]`), env("edit_records"), "sha256:"+strings.Repeat("ff", 32), "w", 0)
		if err == nil || !strings.Contains(err.Error(), "hash mismatch") {
			t.Fatalf("a wrong module hash must fail the commit: %v", err)
		}
		if _, found, _ := s.Head("edits"); found {
			t.Fatal("a refused commit wrote a record")
		}
	})
}

// checkChained asserts a pointer's whole log is append-only and
// chained, and that every record names a durable blob.
func checkChained(t *testing.T, s *Store, name string) Head {
	t.Helper()
	head, _, _ := s.Head(name)
	entries, _, err := s.Log(name, "")
	if err != nil {
		t.Fatal(err)
	}
	if int64(len(entries)) != head.Seq {
		t.Fatalf("log length %d != head seq %d", len(entries), head.Seq)
	}
	for i, e := range entries {
		if e.Seq != int64(i+1) {
			t.Fatalf("seq broken at %d: %+v", i, e)
		}
		if i > 0 && e.From != entries[i-1].To {
			t.Fatalf("chain broken at %d", i)
		}
		if _, _, err := s.GetBlob(e.To); err != nil {
			t.Fatalf("record %d names a missing blob: %v", e.Seq, err)
		}
	}
	return head
}

// race runs writers committing from the head each one reads, over the
// stores given (writer w uses stores[w % len]); a conflict is the
// expected loss, anything else fails.
func race(t *testing.T, stores []*Store, writers, commits int) {
	t.Helper()
	if _, _, err := stores[0].CAS("edits", "", []byte(`[]`), env("edit_records"), "", "seed", 0); err != nil {
		t.Fatal(err)
	}
	var wg sync.WaitGroup
	for w := 0; w < writers; w++ {
		wg.Add(1)
		go func(w int) {
			defer wg.Done()
			s := stores[w%len(stores)]
			for i := 0; i < commits; i++ {
				head, _, err := s.Head("edits")
				if err != nil {
					t.Error(err)
					return
				}
				doc := []byte(`[{"w":` + string(rune('0'+w)) + `,"i":` + string(rune('0'+i%10)) + `}]`)
				if _, _, err := s.CAS("edits", head.Hash, doc, env("edit_records"), "", "w", int64(i)); err != nil {
					var ce *ConflictError
					if !errors.As(err, &ce) {
						t.Error(err)
						return
					}
				}
			}
		}(w)
	}
	wg.Wait()
}

func TestConcurrentWritersConvergeOnOneHead(t *testing.T) {
	each(t, func(t *testing.T, _ Backend, s *Store) {
		race(t, []*Store{s}, 8, 10)
		head := checkChained(t, s, "edits")
		t.Logf("%d commits, head %s", head.Seq, head.Hash)
	})
}

// Several server processes may share one bucket: the conditional create
// of record N+1 is the CAS, so two stores racing commits converge on one
// chained log — no lock (go test -race).
func TestTwoStoresOnOneBucketConvergeOnOneChainedLog(t *testing.T) {
	eachBackend(t, func(t *testing.T, b Backend) {
		a, c := New(b), New(b)
		race(t, []*Store{a, c}, 8, 10)
		// Past the freshness window, both read the bucket's head.
		later := func() time.Time { return time.Now().Add(FreshnessWindow + time.Second) }
		a.now, c.now = later, later
		ha := checkChained(t, a, "edits")
		hc := checkChained(t, c, "edits")
		if ha != hc {
			t.Fatalf("the two stores disagree on the head: %+v vs %+v", ha, hc)
		}
		t.Logf("%d commits across two stores", ha.Seq)
	})
}

// A store sees another process's commit: a read is at most the
// freshness window behind it, and a commit from the newer head — one the
// writer read through the other process — lands without waiting.
func TestAStoreSeesAnotherProcesssCommit(t *testing.T) {
	eachBackend(t, func(t *testing.T, b Backend) {
		a, c := New(b), New(b)
		h1, _, err := a.CAS("edits", "", []byte(`[1]`), env("edit_records"), "", "a", 1)
		if err != nil {
			t.Fatal(err)
		}
		if head, found, _ := c.Head("edits"); !found || head.Hash != h1 {
			t.Fatalf("a first read lists the bucket: %+v", head)
		}
		h2, _, err := a.CAS("edits", h1, []byte(`[2]`), env("edit_records"), "", "a", 2)
		if err != nil {
			t.Fatal(err)
		}
		// Inside the window c may still answer h1; past it, h2.
		c.now = func() time.Time { return time.Now().Add(FreshnessWindow + time.Second) }
		if head, _, _ := c.Head("edits"); head.Hash != h2 || head.Seq != 2 {
			t.Fatalf("past the window the head is the other process's: %+v", head)
		}
		// A writer that read h3 through a commits from it on c at once.
		h3, _, _ := a.CAS("edits", h2, []byte(`[3]`), env("edit_records"), "", "a", 3)
		c.now = time.Now
		if _, seq, err := c.CAS("edits", h3, []byte(`[4]`), env("edit_records"), "", "c", 4); err != nil || seq != 4 {
			t.Fatalf("a commit from a newer head than the cache: seq=%d %v", seq, err)
		}
	})
}

func TestGetBlobRoundTrip(t *testing.T) {
	each(t, func(t *testing.T, _ Backend, s *Store) {
		h, err := s.PutBlob([]byte(`{"a":1}`), env("manifest"), "")
		if err != nil {
			t.Fatal(err)
		}
		data, gotEnv, err := s.GetBlob(h)
		if err != nil || string(data) != `{"a":1}` || gotEnv.Kind != "manifest" {
			t.Fatalf("getblob: %q %+v %v", data, gotEnv, err)
		}
		// Idempotent put returns the same hash.
		h2, err := s.PutBlob([]byte(`{"a":1}`), env("manifest"), "")
		if err != nil || h2 != h {
			t.Fatalf("put idempotent: %s vs %s (%v)", h, h2, err)
		}
		if _, _, err := s.GetBlob("sha256:" + strings.Repeat("00", 32)); !errors.Is(err, ErrNotFound) {
			t.Fatalf("missing blob: %v", err)
		}
	})
}

// The GC cadence: a store that stays open (the server runs for weeks)
// still collects crash-torn orphans — a periodic in-process sweep. The
// sweep keeps every guarantee of GC: it never removes a referenced blob
// or one inside the grace period, and commits racing it stay intact
// (go test -race). Memory only: it backdates one orphan.
func TestSweepCollectsAgedOrphansWhileOpen(t *testing.T) {
	m := NewMemory()
	s := New(m)
	h1, _, err := s.CAS("edits", "", []byte(`[1]`), env("edit_records"), "", "w", 0)
	if err != nil {
		t.Fatal(err)
	}
	aged, _ := s.PutBlob([]byte(`["aged"]`), env("edit_records"), "")
	fresh, _ := s.PutBlob([]byte(`["fresh"]`), env("edit_records"), "")
	h, _ := ParseHash(aged)
	m.age("blobs/"+h, 48*time.Hour)

	stop := s.Sweep(5*time.Millisecond, OrphanGraceMs)
	defer stop()

	// Commits race the sweep: every committed head stays readable.
	var wg sync.WaitGroup
	for w := 0; w < 4; w++ {
		wg.Add(1)
		go func(w int) {
			defer wg.Done()
			for i := 0; i < 20; i++ {
				head, _, _ := s.Head("edits")
				body := []byte(`[` + strings.Repeat("0", w+1) + `,` + strings.Repeat("1", i+1) + `]`)
				s.CAS("edits", head.Hash, body, env("edit_records"), "", "w", int64(i))
			}
		}(w)
	}
	wg.Wait()

	deadline := time.Now().Add(2 * time.Second)
	for {
		if _, _, err := s.GetBlob(aged); errors.Is(err, ErrNotFound) {
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("the sweep never collected the aged orphan")
		}
		time.Sleep(5 * time.Millisecond)
	}
	if _, _, err := s.GetBlob(fresh); err != nil {
		t.Fatalf("an orphan inside the grace period must survive the sweep: %v", err)
	}
	if _, _, err := s.GetBlob(h1); err != nil {
		t.Fatalf("a referenced blob must survive the sweep: %v", err)
	}
	checkChained(t, s, "edits")
	stop()
	stop() // idempotent
}

// A pointer name is one key segment under pointers/: only names that
// cannot reach another pointer's records (or hide as a dotfile) are
// pointers. Plan ids become pointer names (plan.<id>), so the store
// refuses the rest rather than trusting every caller to.
func TestPointerNamesStayInsideTheStore(t *testing.T) {
	each(t, func(t *testing.T, _ Backend, s *Store) {
		for _, bad := range []string{"", ".", "..", "../x", "a/b", `a\b`, ".hidden", "a b", strings.Repeat("x", 129)} {
			if _, _, err := s.CAS(bad, "", []byte(`[]`), env("edit_records"), "", "w", 0); err == nil {
				t.Fatalf("pointer name %q must be refused", bad)
			}
			if _, found, err := s.Head(bad); found || err != nil {
				t.Fatalf("pointer name %q has no head: %v %v", bad, found, err)
			}
		}
		for _, good := range []string{"edits", "manifest", "plans", "plan.3f2c9a1e-0b7d-4c55-9e0a-1d2f3a4b5c6d", "plan.fall_2026"} {
			if _, _, err := s.CAS(good, "", []byte(`[]`), env("edit_records"), "", "w", 0); err != nil {
				t.Fatalf("pointer name %q: %v", good, err)
			}
		}
	})
}

// A store URL names its backend; the directory store is gone.
func TestOpenNamesItsBackend(t *testing.T) {
	s, err := Open("mem://")
	if err != nil {
		t.Fatal(err)
	}
	if _, _, err := s.CAS("edits", "", []byte(`[]`), env("edit_records"), "", "w", 0); err != nil {
		t.Fatal(err)
	}
	for _, bad := range []string{"", "./store", "/var/store", "file:///x", "s3://", "ftp://x/y"} {
		if _, err := Open(bad); err == nil {
			t.Fatalf("store URL %q must be refused", bad)
		}
	}
}
