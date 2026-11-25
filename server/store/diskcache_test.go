package store

// The disk cache (D-224; docs/architecture/server.md, "The disk cache").

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"testing"
	"time"
)

// A second cachedStore over the same dir is the same copy started again.
func cachedStore(t *testing.T, b Backend, dir string) *Store {
	t.Helper()
	s := NewServing(b)
	if err := s.KeepCache(dir, "mem://test"); err != nil {
		t.Fatal(err)
	}
	return s
}

func TestACopyStartedWhileTheBucketIsDownServesItsCacheStaleAndRefusesWrites(t *testing.T) {
	f := &flaky{Backend: NewMemory()}
	dir := t.TempDir()
	first := cachedStore(t, f, dir)
	h1, _, err := first.CAS("manifest", "", []byte(`{"m":1}`), env("manifest"), "", "w", 1000)
	if err != nil {
		t.Fatal(err)
	}
	for seq, text := range []string{`{"e":1}`, `{"e":2}`, `{"e":3}`} {
		if err := first.AppendEntry("disruptions", int64(seq+1), []byte(text)); err != nil {
			t.Fatal(err)
		}
	}
	if err := first.PutCheckpoint("disruptions", 2, []byte(`{"cp":2}`)); err != nil {
		t.Fatal(err)
	}
	confirmed, _, _ := first.Head("manifest")
	logConfirmed, _ := first.LogHeadOf("disruptions")
	if _, err := first.LogHeadOf("never-written"); err != nil {
		t.Fatal(err)
	}

	f.down.Store(true)
	again := cachedStore(t, f, dir)
	head, found, err := again.Head("manifest")
	if err != nil || !found || head.Hash != h1 || head.Seq != 1 || !head.Stale || head.AtMs != 1000 || head.AsOfMs != confirmed.AsOfMs {
		t.Fatalf("the head it last confirmed, stale, as of then: %+v %v", head, err)
	}
	if data, e, err := again.GetBlob(h1); err != nil || string(data) != `{"m":1}` || e.Kind != "manifest" {
		t.Fatalf("the cached blob: %q %v", data, err)
	}
	lh, err := again.LogHeadOf("disruptions")
	if err != nil || lh.Seq != 3 || lh.Checkpoint != 2 || !lh.Stale || lh.AsOfMs != logConfirmed.AsOfMs {
		t.Fatalf("the log head it last confirmed, stale: %+v %v", lh, err)
	}
	if empty, err := again.LogHeadOf("never-written"); err != nil || empty.Seq != 0 || !empty.Stale {
		t.Fatalf("an empty log, confirmed empty, is stale and empty: %+v %v", empty, err)
	}
	if _, err := again.LogHeadOf("never-read"); !errors.Is(err, ErrUnavailable) {
		t.Fatalf("a log the copy never read has nothing to serve: %v", err)
	}
	if cps, err := again.Checkpoints("disruptions"); err != nil || len(cps) != 1 || cps[0] != 2 {
		t.Fatalf("the cached checkpoints: %v %v", cps, err)
	}
	if cp, err := again.Checkpoint("disruptions", 2); err != nil || string(cp) != `{"cp":2}` {
		t.Fatalf("the cached checkpoint: %q %v", cp, err)
	}
	if es, err := again.Entries("disruptions", 2, 0); err != nil || len(es) != 1 || es[0].Seq != 3 || es[0].Text != `{"e":3}` {
		t.Fatalf("the cached entries after the checkpoint: %+v %v", es, err)
	}
	if _, _, err := again.CAS("manifest", h1, []byte(`{"m":2}`), env("manifest"), "", "w", 2000); !errors.Is(err, ErrUnavailable) {
		t.Fatalf("a pointer write is refused as unavailable: %v", err)
	}
	if err := again.AppendEntry("disruptions", 4, []byte(`{"e":4}`)); !errors.Is(err, ErrUnavailable) {
		t.Fatalf("a log write is refused as unavailable: %v", err)
	}

	f.down.Store(false)
	head, _, err = again.Head("manifest")
	if err != nil || head.Stale || head.Hash != h1 {
		t.Fatalf("back up: the bucket's head, fresh: %+v %v", head, err)
	}
	if lh, err := again.LogHeadOf("disruptions"); err != nil || lh.Stale || lh.Seq != 3 {
		t.Fatalf("back up: the bucket's log head: %+v %v", lh, err)
	}
	if err := again.AppendEntry("disruptions", 4, []byte(`{"e":4}`)); err != nil {
		t.Fatalf("the resent write lands: %v", err)
	}
	if _, seq, err := again.CAS("manifest", h1, []byte(`{"m":2}`), env("manifest"), "", "w", 3000); err != nil || seq != 2 {
		t.Fatalf("the resent commit lands: %d %v", seq, err)
	}
}

// A restored older bucket wins over the cache's newer head.
func TestTheBucketWinsOverTheCacheWhenItAnswers(t *testing.T) {
	dir := t.TempDir()
	newer := &flaky{Backend: NewMemory()}
	s := cachedStore(t, newer, dir)
	h1, _, _ := s.CAS("manifest", "", []byte(`1`), env("manifest"), "", "w", 1)
	s.CAS("manifest", h1, []byte(`2`), env("manifest"), "", "w", 2)
	s.AppendEntry("disruptions", 1, []byte(`a`))
	s.AppendEntry("disruptions", 2, []byte(`b`))
	s.LogHeadOf("disruptions")

	restored := &flaky{Backend: NewMemory()}
	New(restored).CAS("manifest", "", []byte(`1`), env("manifest"), "", "w", 1)
	New(restored).AppendEntry("disruptions", 1, []byte(`a`))

	restored.down.Store(true)
	c := cachedStore(t, restored, dir)
	if head, _, _ := c.Head("manifest"); head.Seq != 2 || !head.Stale {
		t.Fatalf("while down, the cache's head: %+v", head)
	}
	if lh, _ := c.LogHeadOf("disruptions"); lh.Seq != 2 || !lh.Stale {
		t.Fatalf("while down, the cache's log head: %+v", lh)
	}
	restored.down.Store(false)
	if head, _, err := c.Head("manifest"); err != nil || head.Seq != 1 || head.Hash != h1 || head.Stale {
		t.Fatalf("the bucket's head, not the cache's: %+v %v", head, err)
	}
	if lh, err := c.LogHeadOf("disruptions"); err != nil || lh.Seq != 1 || lh.Stale {
		t.Fatalf("the bucket's log head, not the cache's: %+v %v", lh, err)
	}
	if err := c.AppendEntry("disruptions", 2, []byte(`c`)); err != nil {
		t.Fatalf("the next write goes to the bucket's next seq: %v", err)
	}
}

func TestTheCacheIsScopedToItsStore(t *testing.T) {
	dir := t.TempDir()
	f := &flaky{Backend: NewMemory()}
	a := NewServing(f)
	a.KeepCache(dir, "s3://a/x")
	a.CAS("manifest", "", []byte(`1`), env("manifest"), "", "w", 1)

	f.down.Store(true)
	b := NewServing(f)
	b.KeepCache(dir, "s3://b/x")
	if _, _, err := b.Head("manifest"); !errors.Is(err, ErrUnavailable) {
		t.Fatalf("another store's cache is not served: %v", err)
	}
	if url, _ := os.ReadFile(filepath.Join(dir, CacheScope("s3://a/x"), "store-url")); string(url) != "s3://a/x\n" {
		t.Fatalf("each scope names its store: %q", url)
	}
}

func TestTheCacheIsBounded(t *testing.T) {
	dir := t.TempDir()
	f := &flaky{Backend: NewMemory()}
	s := cachedStore(t, f, dir)
	s.disk.max = 20
	a, _ := s.PutBlob([]byte(`"aaaaaa"`), env("manifest"), "")
	b, _ := s.PutBlob([]byte(`"bbbbbb"`), env("manifest"), "")
	time.Sleep(10 * time.Millisecond)
	s.GetBlob(a) // a is used after b
	time.Sleep(10 * time.Millisecond)
	c, _ := s.PutBlob([]byte(`"cccccc"`), env("manifest"), "")
	if _, _, ok := s.disk.getBlob(b); ok {
		t.Fatal("the least recently used blob goes first")
	}
	for _, h := range []string{a, c} {
		if _, _, ok := s.disk.getBlob(h); !ok {
			t.Fatalf("%s stays", h)
		}
	}

	for seq := int64(1); seq <= 6; seq++ {
		s.AppendEntry("disruptions", seq, []byte(`e`))
	}
	for _, cp := range []int64{2, 4, 5} {
		if err := s.PutCheckpoint("disruptions", cp, []byte(`cp`)); err != nil {
			t.Fatal(err)
		}
	}
	if got := s.disk.logSeqs("disruptions", "checkpoints"); len(got) != 2 || got[0] != 4 || got[1] != 5 {
		t.Fatalf("the newest two checkpoints: %v", got)
	}
	if got := s.disk.logSeqs("disruptions", "entries"); len(got) != 2 || got[0] != 5 || got[1] != 6 {
		t.Fatalf("the entries after the older kept checkpoint: %v", got)
	}
}

func TestACacheFolderThatCannotBeWrittenFailsAtBoot(t *testing.T) {
	file := filepath.Join(t.TempDir(), "a-file")
	os.WriteFile(file, nil, 0o644)
	if err := NewServing(NewMemory()).KeepCache(file, "mem://"); err == nil {
		t.Fatal("a file is not a cache folder")
	}
}

// A call whose context ends first has done nothing, like a request that
// the network dropped.
type slow struct {
	Backend
	delay time.Duration
}

func (s *slow) wait(ctx context.Context) error {
	select {
	case <-time.After(s.delay):
		return nil
	case <-ctx.Done():
		return ctx.Err()
	}
}
func (s *slow) Get(ctx context.Context, k string) ([]byte, map[string]string, error) {
	if err := s.wait(ctx); err != nil {
		return nil, nil, err
	}
	return s.Backend.Get(ctx, k)
}
func (s *slow) Stat(ctx context.Context, k string) (ObjectInfo, error) {
	if err := s.wait(ctx); err != nil {
		return ObjectInfo{}, err
	}
	return s.Backend.Stat(ctx, k)
}
func (s *slow) Create(ctx context.Context, k string, d []byte, m map[string]string) error {
	if err := s.wait(ctx); err != nil {
		return err
	}
	return s.Backend.Create(ctx, k, d, m)
}
func (s *slow) List(ctx context.Context, p, a string) ([]ObjectInfo, error) {
	if err := s.wait(ctx); err != nil {
		return nil, err
	}
	return s.Backend.List(ctx, p, a)
}

// A call past its timeout is a bucket that does not answer (D-119).
func TestASlowBucketPastTheTimeoutIsABucketThatDoesNotAnswer(t *testing.T) {
	call, create := callTimeout, createTimeout
	callTimeout, createTimeout = 50*time.Millisecond, 50*time.Millisecond
	t.Cleanup(func() { callTimeout, createTimeout = call, create })

	b := &slow{Backend: NewMemory()}
	s := New(b)
	h1, _, err := s.CAS("manifest", "", []byte(`1`), env("manifest"), "", "w", 1)
	if err != nil {
		t.Fatal(err)
	}
	if err := s.AppendEntry("disruptions", 1, []byte(`a`)); err != nil {
		t.Fatal(err)
	}

	b.delay = 10 * time.Millisecond // slow, within the timeout
	s.now = func() time.Time { return time.Now().Add(time.Hour) }
	if head, _, err := s.Head("manifest"); err != nil || head.Stale || head.Hash != h1 {
		t.Fatalf("a slow answer is an answer: %+v %v", head, err)
	}

	b.delay = time.Second // past the timeout
	s.now = func() time.Time { return time.Now().Add(2 * time.Hour) }
	started := time.Now()
	if head, _, err := s.Head("manifest"); err != nil || !head.Stale || head.Hash != h1 {
		t.Fatalf("past the timeout, the cached head, stale: %+v %v", head, err)
	}
	if lh, err := s.LogHeadOf("disruptions"); err != nil || !lh.Stale || lh.Seq != 1 {
		t.Fatalf("past the timeout, the cached log head, stale: %+v %v", lh, err)
	}
	if _, _, err := s.CAS("manifest", h1, []byte(`2`), env("manifest"), "", "w", 2); !errors.Is(err, ErrUnavailable) {
		t.Fatalf("a write past the timeout is unavailable: %v", err)
	}
	if err := s.AppendEntry("disruptions", 2, []byte(`b`)); !errors.Is(err, ErrUnavailable) {
		t.Fatalf("an entry past the timeout is unavailable: %v", err)
	}
	if took := time.Since(started); took > 900*time.Millisecond {
		t.Fatalf("each call ends at its timeout, not the bucket's answer: %v", took)
	}

	b.delay = 0
	if _, seq, err := s.CAS("manifest", h1, []byte(`2`), env("manifest"), "", "w", 3); err != nil || seq != 2 {
		t.Fatalf("fast again, the resent commit lands once: %d %v", seq, err)
	}
	if err := s.AppendEntry("disruptions", 2, []byte(`b`)); err != nil {
		t.Fatalf("fast again, the resent entry lands: %v", err)
	}
	if es, _ := s.Entries("disruptions", 0, 0); len(es) != 2 {
		t.Fatalf("each entry once: %+v", es)
	}
}
