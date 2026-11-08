package main

// Degraded mode (D-119), the Go round trip: while the bucket does not
// answer, the server serves the cached heads — each read labelled with
// X-Store-Stale and X-Store-As-Of — and refuses every write with 503 and
// Retry-After, acknowledging nothing; the client holds its document and
// resends, and the write lands once the bucket answers.

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"rustwasm/server/store"
)

type flakyBucket struct {
	store.Backend
	down atomic.Bool
}

var errBucketDown = errors.New("dial tcp: connection refused")

func (f *flakyBucket) Get(ctx context.Context, k string) ([]byte, map[string]string, error) {
	if f.down.Load() {
		return nil, nil, errBucketDown
	}
	return f.Backend.Get(ctx, k)
}
func (f *flakyBucket) Stat(ctx context.Context, k string) (store.ObjectInfo, error) {
	if f.down.Load() {
		return store.ObjectInfo{}, errBucketDown
	}
	return f.Backend.Stat(ctx, k)
}
func (f *flakyBucket) Create(ctx context.Context, k string, d []byte, m map[string]string) error {
	if f.down.Load() {
		return errBucketDown
	}
	return f.Backend.Create(ctx, k, d, m)
}
func (f *flakyBucket) List(ctx context.Context, p, a string) ([]store.ObjectInfo, error) {
	if f.down.Load() {
		return nil, errBucketDown
	}
	return f.Backend.List(ctx, p, a)
}

func TestABucketThatStopsAnsweringServesLabelledReadsAndRefusesWrites(t *testing.T) {
	bucket := &flakyBucket{Backend: store.NewMemory()}
	s := newStoreServer(t, bucket)
	mux := storeMux(s)
	base := httptest.NewServer(mux).URL

	doc := editDoc("e1", "draft", 1, 100)
	if code, _, body := httpPost(t, base+"/edits", doc); code != 200 {
		t.Fatalf("post: %d %s", code, body)
	}
	_, _, before := httpGet(t, base+"/edits")
	code, hdr, manifest := httpGet(t, base+"/manifest")
	if code != 200 || hdr.Get("X-Store-Stale") != "" {
		t.Fatalf("a fresh manifest is unlabelled: %d %v", code, hdr)
	}

	bucket.down.Store(true)
	time.Sleep(store.FreshnessWindow + 100*time.Millisecond) // a probe is due

	code, hdr, after := httpGet(t, base+"/edits")
	if code != 200 || after != before || hdr.Get("X-Store-Stale") != "true" {
		t.Fatalf("the cached edit document, labelled: %d %v", code, hdr)
	}
	asOf, err := strconv.ParseInt(hdr.Get("X-Store-As-Of"), 10, 64)
	if err != nil || asOf <= 0 || asOf > time.Now().UnixMilli() {
		t.Fatalf("X-Store-As-Of is when the bucket last confirmed it: %q", hdr.Get("X-Store-As-Of"))
	}
	if code, hdr, body := httpGet(t, base+"/manifest"); code != 200 || body != manifest || hdr.Get("X-Store-Stale") != "true" {
		t.Fatalf("the cached manifest, labelled: %d %v", code, hdr)
	}
	if code, hdr, body := httpGet(t, base+"/pointers/edits"); code != 200 || !strings.Contains(body, `"stale":true`) || hdr.Get("X-Store-Stale") != "true" {
		t.Fatalf("the pointer's head says stale: %d %s", code, body)
	}

	held := editDoc("e2", "draft", 1, 200)
	code, hdr, body := httpPost(t, base+"/edits", held)
	if code != http.StatusServiceUnavailable || hdr.Get("Retry-After") == "" {
		t.Fatalf("a write while the bucket is down: %d %v %s", code, hdr, body)
	}

	bucket.down.Store(false)
	code, _, body = httpPost(t, base+"/edits", held) // the client resends what it held
	if code != 200 || !strings.Contains(body, `"e1"`) || !strings.Contains(body, `"e2"`) {
		t.Fatalf("the held edit lands once the bucket answers: %d %s", code, body)
	}
	if _, hdr, _ := httpGet(t, base+"/edits"); hdr.Get("X-Store-Stale") != "" {
		t.Fatalf("back up, reads are unlabelled: %v", hdr)
	}
	log, _, _ := s.store.Log("edits", "")
	for _, e := range log {
		if e.AtMs == 0 {
			t.Fatalf("a record without its time: %+v", e)
		}
	}
	if len(log) != 3 { // seed, e1, e2 — the refused write left nothing
		t.Fatalf("the refused write was never committed: %d records", len(log))
	}
}
