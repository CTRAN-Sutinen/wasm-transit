package store

import (
	"context"
	"errors"
	"sync/atomic"
	"testing"
	"time"
)

// flaky is a backend that stops answering on demand: every call fails
// as a network error would, never as ErrNotFound or ErrExists.
type flaky struct {
	Backend
	down atomic.Bool
}

var errDown = errors.New("dial tcp: connection refused")

func (f *flaky) Get(ctx context.Context, k string) ([]byte, map[string]string, error) {
	if f.down.Load() {
		return nil, nil, errDown
	}
	return f.Backend.Get(ctx, k)
}
func (f *flaky) Stat(ctx context.Context, k string) (ObjectInfo, error) {
	if f.down.Load() {
		return ObjectInfo{}, errDown
	}
	return f.Backend.Stat(ctx, k)
}
func (f *flaky) Create(ctx context.Context, k string, d []byte, m map[string]string) error {
	if f.down.Load() {
		return errDown
	}
	return f.Backend.Create(ctx, k, d, m)
}
func (f *flaky) List(ctx context.Context, p, a string) ([]ObjectInfo, error) {
	if f.down.Load() {
		return nil, errDown
	}
	return f.Backend.List(ctx, p, a)
}

// D-119: while the bucket does not answer, a store serves its cached
// heads — each labelled stale, with when the bucket last confirmed it —
// and its cached blobs (immutable, so always correct), and refuses every
// write as unavailable: never a conflict, never acknowledged.
func TestABucketThatStopsAnsweringServesCachedHeadsLabelledStale(t *testing.T) {
	f := &flaky{Backend: NewMemory()}
	s := New(f)
	h1, _, err := s.CAS("edits", "", []byte(`[1]`), env("edit_records"), "", "w", 1000)
	if err != nil {
		t.Fatal(err)
	}
	fresh, _, _ := s.Head("edits")
	if fresh.Stale || fresh.AtMs != 1000 || fresh.AsOfMs == 0 {
		t.Fatalf("a fresh head: %+v", fresh)
	}

	f.down.Store(true)
	s.now = func() time.Time { return time.Now().Add(time.Hour) } // past the window: a probe is due
	head, found, err := s.Head("edits")
	if err != nil || !found || head.Hash != h1 || !head.Stale || head.AsOfMs != fresh.AsOfMs || head.AtMs != 1000 {
		t.Fatalf("the cached head, labelled stale as of its last confirmation: %+v %v", head, err)
	}
	if data, e, err := s.GetBlob(h1); err != nil || string(data) != `[1]` || e.Kind != "edit_records" {
		t.Fatalf("a cached blob is served: %q %v", data, err)
	}
	_, _, err = s.CAS("edits", h1, []byte(`[2]`), env("edit_records"), "", "w", 2000)
	var ce *ConflictError
	if !errors.Is(err, ErrUnavailable) || errors.As(err, &ce) {
		t.Fatalf("a write while down is unavailable, not a conflict: %v", err)
	}

	f.down.Store(false)
	head, _, err = s.Head("edits")
	if err != nil || head.Stale || head.Hash != h1 || head.Seq != 1 {
		t.Fatalf("back up: fresh, and the refused write never landed: %+v %v", head, err)
	}
	if _, seq, err := s.CAS("edits", h1, []byte(`[2]`), env("edit_records"), "", "w", 3000); err != nil || seq != 2 {
		t.Fatalf("the resent write lands: seq=%d %v", seq, err)
	}
}

// A store with nothing cached has nothing to serve: unavailable.
func TestABucketDownBeforeAnyReadIsUnavailable(t *testing.T) {
	f := &flaky{Backend: NewMemory()}
	New(f).CAS("edits", "", []byte(`[1]`), env("edit_records"), "", "w", 1)
	f.down.Store(true)
	s := New(f)
	if _, _, err := s.Head("edits"); !errors.Is(err, ErrUnavailable) {
		t.Fatalf("no cached head: %v", err)
	}
	if _, _, err := s.GetBlob(HashOf([]byte(`[1]`))); !errors.Is(err, ErrUnavailable) {
		t.Fatalf("no cached blob: %v", err)
	}
}

// The blob cache is bounded by bytes: the oldest entries go first, and
// an evicted blob is read from the bucket again.
func TestTheBlobCacheIsBounded(t *testing.T) {
	f := &flaky{Backend: NewMemory()}
	s := New(f)
	s.cache = newBlobCache(10)
	a, _ := s.PutBlob([]byte(`"aaaaaa"`), env("manifest"), "") // 8 bytes
	b, _ := s.PutBlob([]byte(`"bbbbbb"`), env("manifest"), "")
	f.down.Store(true)
	if _, _, err := s.GetBlob(b); err != nil {
		t.Fatalf("the newest blob is cached: %v", err)
	}
	if _, _, err := s.GetBlob(a); !errors.Is(err, ErrUnavailable) {
		t.Fatalf("the oldest was evicted past the bound: %v", err)
	}
}
