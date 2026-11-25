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

// each runs f over every backend (contract_test.go).
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
		head, _, _ = s.Head("edits")
		if head.Hash != h2 || head.Seq != 2 {
			t.Fatalf("failed CAS moved the pointer: %+v", head)
		}
	})
}

// Pins the at-rest layout (docs/FORMATS.md).
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
		// Redelivery of the head bytes is a no-op.
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
		h2, _, err := s.CAS("edits", h1, []byte(`[{"id":"e1","x":1}]`), env("edit_records"), "", "bob", 2000)
		if err != nil {
			t.Fatal(err)
		}
		delta, found, err := s.Log("edits", entries[0].To)
		if err != nil || !found || len(delta) != 1 || delta[0].To != h2 || delta[0].From != h1 {
			t.Fatalf("delta: %v found=%v %+v", err, found, delta)
		}
		_, found, err = s.Log("edits", "sha256:"+strings.Repeat("00", 32))
		if err != nil || found {
			t.Fatalf("unknown since must be found=false: %v %v", found, err)
		}
		if _, found, err := s.Log("nothing", ""); err != nil || found {
			t.Fatalf("no pointer, no log: %v %v", found, err)
		}
	})
}

// A delta from a hash that the pointer held twice starts after the
// latest time it held it.
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
		orphan, err := s.PutBlob([]byte(`[2]`), env("edit_records"), "")
		if err != nil {
			t.Fatal(err)
		}

		s2 := New(b)
		head, found, _ := s2.Head("edits")
		if !found || head.Hash != h1 || head.Seq != 1 {
			t.Fatalf("crash must leave the old authority readable: %+v", head)
		}
		if r, err := s2.GC(OrphanGraceMs); err != nil || r.Removed != 0 {
			t.Fatalf("an orphan inside the grace survives: %+v %v", r, err)
		}
		if _, _, err := s2.GetBlob(orphan); err != nil {
			t.Fatalf("orphan blob should still exist within grace: %v", err)
		}
		s2.now = func() time.Time { return time.Now().Add(OrphanGrace + time.Hour) }
		r, err := s2.GC(OrphanGraceMs)
		if err != nil || r.Removed != 1 || r.Restored != 0 {
			t.Fatalf("gc: %+v err=%v", r, err)
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

// race commits from the head that each writer reads. Writer w uses
// stores[w % len]. A conflict is expected, and anything else fails.
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

// Two stores racing commits over one bucket converge on one chained
// log, with no lock.
func TestTwoStoresOnOneBucketConvergeOnOneChainedLog(t *testing.T) {
	eachBackend(t, func(t *testing.T, b Backend) {
		a, c := New(b), New(b)
		race(t, []*Store{a, c}, 8, 10)
		later := func() time.Time { return time.Now().Add(FreshnessWindow + time.Second) }
		a.now, c.now = later, later
		ha := checkChained(t, a, "edits")
		hc := checkChained(t, c, "edits")
		ha.AsOfMs, hc.AsOfMs = 0, 0 // each process confirms at its own time
		if ha != hc {
			t.Fatalf("the two stores disagree on the head: %+v vs %+v", ha, hc)
		}
		t.Logf("%d commits across two stores", ha.Seq)
	})
}

// A read is at most the freshness window behind another process's
// commit, and a commit from the newer head lands without waiting.
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
		c.now = func() time.Time { return time.Now().Add(FreshnessWindow + time.Second) }
		if head, _, _ := c.Head("edits"); head.Hash != h2 || head.Seq != 2 {
			t.Fatalf("past the window the head is the other process's: %+v", head)
		}
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
		h2, err := s.PutBlob([]byte(`{"a":1}`), env("manifest"), "")
		if err != nil || h2 != h {
			t.Fatalf("put idempotent: %s vs %s (%v)", h, h2, err)
		}
		if _, _, err := s.GetBlob("sha256:" + strings.Repeat("00", 32)); !errors.Is(err, ErrNotFound) {
			t.Fatalf("missing blob: %v", err)
		}
	})
}

// hooked runs a hook after each create lands, to stand in for another
// process between two of this one's steps.
type hooked struct {
	Backend
	afterCreate func(key string)
}

func (h hooked) Create(ctx context.Context, key string, data []byte, meta map[string]string) error {
	err := h.Backend.Create(ctx, key, data, meta)
	if err == nil && h.afterCreate != nil {
		h.afterCreate(key)
	}
	return err
}

// A commit re-creates its blob if a GC pass deleted it after the record
// landed (store.tla).
func TestACommitRecreatesItsBlobIfItVanishesAfterItsRecord(t *testing.T) {
	eachBackend(t, func(t *testing.T, b Backend) {
		ctx := context.Background()
		doc := []byte(`["reused"]`)
		h, _ := ParseHash(hashOf(doc))
		s := New(hooked{b, func(key string) {
			if strings.HasPrefix(key, "pointers/") {
				b.Delete(ctx, "blobs/"+h) // GC, between the record and the re-read
			}
		}})
		hash, _, err := s.CAS("edits", "", doc, env("edit_records"), "", "w", 1)
		if err != nil {
			t.Fatal(err)
		}
		data, gotEnv, err := s.GetBlob(hash)
		if err != nil || string(data) != string(doc) || gotEnv != env("edit_records") {
			t.Fatalf("the commit re-created its blob with its envelope: %q %+v %v", data, gotEnv, err)
		}
	})
}

// The race that store.tla's Settle closes (trace: store-race.cfg).
func TestGCRestoresABlobACommitNamedDuringThePass(t *testing.T) {
	each(t, func(t *testing.T, b Backend, s *Store) {
		doc := []byte(`["orphan"]`)
		orphan, err := s.PutBlob(doc, env("edit_records"), "")
		if err != nil {
			t.Fatal(err)
		}
		gc := New(b)
		gc.now = func() time.Time { return time.Now().Add(OrphanGrace + time.Hour) }
		gc.afterList = func() {
			if _, _, err := s.CAS("edits", "", doc, env("edit_records"), "", "w", 1); err != nil {
				t.Fatal(err)
			}
		}
		r, err := gc.GC(OrphanGraceMs)
		if err != nil || r.Removed != 0 || r.Restored != 1 {
			t.Fatalf("the pass restores what it trashed and a record now names: %+v %v", r, err)
		}
		if _, gotEnv, err := s.GetBlob(orphan); err != nil || gotEnv != env("edit_records") {
			t.Fatalf("the committed blob is durable with its envelope: %+v %v", gotEnv, err)
		}
		if left, _ := b.List(context.Background(), "gc/", ""); len(left) != 0 {
			t.Fatalf("a settled pass leaves no trash: %+v", left)
		}
	})
}

// A pass that crashed after trashing leaves gc/ behind. The next pass
// settles it first.
func TestGCSettlesTheTrashACrashedPassLeft(t *testing.T) {
	each(t, func(t *testing.T, b Backend, s *Store) {
		ctx := context.Background()
		named := []byte(`["named"]`)
		hash, _, err := s.CAS("edits", "", named, env("edit_records"), "", "w", 1)
		if err != nil {
			t.Fatal(err)
		}
		h, _ := ParseHash(hash)
		meta, _ := json.Marshal(env("edit_records"))
		b.Delete(ctx, "blobs/"+h)
		b.Create(ctx, "gc/"+h, named, map[string]string{"envelope": string(meta)})
		b.Create(ctx, "gc/"+strings.Repeat("0", 64), []byte(`["gone"]`), map[string]string{"envelope": string(meta)})

		r, err := New(b).GC(OrphanGraceMs)
		if err != nil || r.Restored != 1 || r.Removed != 0 {
			t.Fatalf("settle: %+v %v", r, err)
		}
		if data, _, err := s.GetBlob(hash); err != nil || string(data) != string(named) {
			t.Fatalf("the named blob is back: %q %v", data, err)
		}
		if left, _ := b.List(ctx, "gc/", ""); len(left) != 0 {
			t.Fatalf("gc/ is empty after a pass: %+v", left)
		}
	})
}

// A serving store never deletes.
func TestAServingStoreNeverDeletes(t *testing.T) {
	b := NewMemory()
	s := NewServing(b)
	if _, _, err := s.CAS("edits", "", []byte(`[]`), env("edit_records"), "", "w", 1); err != nil {
		t.Fatal(err)
	}
	if _, err := s.GC(OrphanGraceMs); err == nil {
		t.Fatal("a serving store must refuse GC")
	}
	if err := s.b.Delete(context.Background(), "blobs/x"); err == nil {
		t.Fatal("a serving store's backend must refuse delete")
	}
}

// Plan ids become pointer names (plan.<id>), so the store refuses a
// name that could reach another pointer's records.
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

// There is no directory backend.
func TestOpenNamesItsBackend(t *testing.T) {
	s, err := Open("mem://")
	if err != nil {
		t.Fatal(err)
	}
	if _, _, err := s.CAS("edits", "", []byte(`[]`), env("edit_records"), "", "w", 0); err != nil {
		t.Fatal(err)
	}
	for _, bad := range []string{"", "./store", "/var/store", "file:///x", "s3://", "azblob://", "gs://", "ftp://x/y"} {
		if _, err := Open(bad); err == nil {
			t.Fatalf("store URL %q must be refused", bad)
		}
	}
}
