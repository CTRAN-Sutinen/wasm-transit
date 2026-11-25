package main

// Pins boot sources (D-118). Every read that lands is committed as the
// head of source.feed or source.streets, so a replacement process with
// only the bucket serves the same manifest even when the source does not
// answer.

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"sync/atomic"
	"testing"

	"rustwasm/server/store"
)

const sampleFeed = "../web/e2e/fixtures/sample-gtfs.zip"

func shaOf(b []byte) string {
	sum := sha256.Sum256(b)
	return "sha256:" + hex.EncodeToString(sum[:])
}

type origin struct {
	srv          *httptest.Server
	body         atomic.Value // []byte
	down         atomic.Bool
	full, cached atomic.Int32
}

func newOrigin(t *testing.T, body []byte) *origin {
	o := &origin{}
	o.body.Store(body)
	o.srv = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if o.down.Load() {
			http.Error(w, "origin down", http.StatusBadGateway)
			return
		}
		b := o.body.Load().([]byte)
		etag := `"` + shaOf(b) + `"`
		if r.Header.Get("If-None-Match") == etag {
			o.cached.Add(1)
			w.WriteHeader(http.StatusNotModified)
			return
		}
		o.full.Add(1)
		w.Header().Set("ETag", etag)
		w.Write(b)
	}))
	t.Cleanup(o.srv.Close)
	return o
}

func bootServer(t *testing.T, bucket store.Backend, src string) *server {
	t.Helper()
	s := newStoreServer(t, bucket)
	s.feeder = &feedSource{src: newSource(src, "source.feed", "gtfs_feed", s.store), networkWasm: networkWasm}
	return s
}

func TestAFeedFromAURLIsImportedAndCommittedWithItsSource(t *testing.T) {
	zip, _ := os.ReadFile(sampleFeed)
	o := newOrigin(t, zip)
	bucket := store.NewMemory()
	s := bootServer(t, bucket, o.srv.URL+"/feed.zip")
	feed, err := s.feeder.Load()
	if err != nil {
		t.Fatal(err)
	}
	if feed.hash != shaOf(zip) || feed.networkHash == "" {
		t.Fatalf("the feed is the URL's bytes: %+v", feed.hash)
	}
	head, found, _ := s.store.Head("source.feed")
	if !found || head.Hash != shaOf(zip) || head.Envelope.Kind != "gtfs_feed" {
		t.Fatalf("the source's bytes are committed: %+v", head)
	}
	log, _, _ := s.store.Log("source.feed", "")
	if len(log) != 1 || log[0].Writer != "import:"+o.srv.URL+"/feed.zip" {
		t.Fatalf("the record names its source: %+v", log)
	}
	s2 := bootServer(t, bucket, o.srv.URL+"/feed.zip")
	if _, err := s2.feeder.Load(); err != nil {
		t.Fatal(err)
	}
	if log, _, _ := s2.store.Log("source.feed", ""); len(log) != 1 {
		t.Fatalf("an unchanged source is not re-committed: %d records", len(log))
	}
}

func TestABootWhoseSourceDoesNotAnswerServesTheLastCommittedImport(t *testing.T) {
	zip, _ := os.ReadFile(sampleFeed)
	o := newOrigin(t, zip)
	bucket := store.NewMemory()
	first := bootServer(t, bucket, o.srv.URL+"/feed.zip")
	want, err := first.buildManifest()
	if err != nil {
		t.Fatal(err)
	}

	o.down.Store(true)
	replacement := bootServer(t, bucket, o.srv.URL+"/feed.zip")
	got, err := replacement.buildManifest()
	if err != nil {
		t.Fatalf("a source that does not answer falls back to the store: %v", err)
	}
	if manifestKeyOf(got) != manifestKeyOf(want) || got.NetworkHash != want.NetworkHash || got.FeedHash != want.FeedHash {
		t.Fatalf("the replacement serves another manifest: %+v vs %+v", got, want)
	}
}

func TestABootWithNoSourceAndNothingCommittedFails(t *testing.T) {
	o := newOrigin(t, []byte("x"))
	o.down.Store(true)
	s := bootServer(t, store.NewMemory(), o.srv.URL+"/feed.zip")
	if _, err := s.feeder.Load(); err == nil || !strings.Contains(err.Error(), "origin") && !strings.Contains(err.Error(), "502") {
		t.Fatalf("no source, no committed import: %v", err)
	}
}

func TestAFeedFromTheBucketIsReadFromInputs(t *testing.T) {
	zip, _ := os.ReadFile(sampleFeed)
	bucket := store.NewMemory()
	if err := bucket.Create(context.Background(), "inputs/gtfs/c-tran.zip", zip, nil); err != nil {
		t.Fatal(err)
	}
	s := bootServer(t, bucket, "bucket:gtfs/c-tran.zip")
	feed, err := s.feeder.Load()
	if err != nil || feed.hash != shaOf(zip) {
		t.Fatalf("bucket source: %v", err)
	}
	if log, _, _ := s.store.Log("source.feed", ""); len(log) != 1 || log[0].Writer != "import:bucket:gtfs/c-tran.zip" {
		t.Fatalf("the record names its source: %+v", log)
	}
}

// Pins that a URL source is re-checked with a conditional GET at most
// once per interval.
func TestAURLSourceIsRecheckedConditionally(t *testing.T) {
	zip, _ := os.ReadFile(sampleFeed)
	o := newOrigin(t, zip)
	s := bootServer(t, store.NewMemory(), o.srv.URL+"/feed.zip")
	s.feeder.src.every = 0
	first, err := s.feeder.Load()
	if err != nil {
		t.Fatal(err)
	}
	again, err := s.feeder.Load()
	if err != nil || again != first || o.full.Load() != 1 || o.cached.Load() != 1 {
		t.Fatalf("an unchanged origin answers 304: full=%d cached=%d same=%v %v", o.full.Load(), o.cached.Load(), again == first, err)
	}
	o.down.Store(true)
	if held, err := s.feeder.Load(); err != nil || held != first {
		t.Fatalf("a source that stops answering keeps the last import: %v", err)
	}
}

func TestThePolicySeedsFromAURL(t *testing.T) {
	policy := testPolicy()
	o := newOrigin(t, policy)
	s := newStoreServer(t, store.NewMemory())
	s.seedPolicy(o.srv.URL + "/policy.json")
	head, found, _ := s.store.Head("policy")
	if !found || head.Hash != shaOf(policy) {
		t.Fatalf("the policy pointer holds the URL's bytes: %+v", head)
	}
}
