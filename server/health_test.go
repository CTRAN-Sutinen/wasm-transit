package main

// Health, readiness, and graceful shutdown: what a load balancer or an
// orchestrator reads. Liveness never reads the bucket; readiness stays
// 200 while degraded reads serve (every instance shares the bucket, so
// pulling them all would turn a write outage into a read outage), is
// 503 with nothing to serve, and is 503 "draining" once shutdown
// starts; a request in flight at shutdown finishes.

import (
	"encoding/json"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"syscall"
	"testing"
	"time"

	"rustwasm/server/store"
)

func readyz(t *testing.T, s *server) (int, readiness) {
	t.Helper()
	rec := httptest.NewRecorder()
	s.handleReadyz(rec, httptest.NewRequest("GET", "/readyz", nil))
	var rd readiness
	if err := json.Unmarshal(rec.Body.Bytes(), &rd); err != nil {
		t.Fatalf("readyz body: %v %q", err, rec.Body.String())
	}
	return rec.Code, rd
}

func TestLivenessAnswersWithoutReadingTheBucket(t *testing.T) {
	bucket := &flakyBucket{Backend: store.NewMemory()}
	s := newStoreServer(t, bucket)
	bucket.down.Store(true)
	rec := httptest.NewRecorder()
	s.handleHealthz(rec, httptest.NewRequest("GET", "/healthz", nil))
	if rec.Code != 200 {
		t.Fatalf("healthz with the bucket down: %d", rec.Code)
	}
}

func TestReadinessWithoutAStoreIsReady(t *testing.T) {
	s := &server{}
	if code, rd := readyz(t, s); code != 200 || rd.Status != "ready" || rd.Store != "none" {
		t.Fatalf("dev store: %d %+v", code, rd)
	}
}

func TestReadinessFollowsTheBucket(t *testing.T) {
	bucket := &flakyBucket{Backend: store.NewMemory()}
	s := newStoreServer(t, bucket)
	if code, rd := readyz(t, s); code != 200 || rd.Status != "ready" || rd.Store != "ok" {
		t.Fatalf("bucket answering: %d %+v", code, rd)
	}

	bucket.down.Store(true)
	time.Sleep(store.FreshnessWindow + 100*time.Millisecond) // a probe is due
	code, rd := readyz(t, s)
	if code != 200 || rd.Status != "degraded" || rd.Store != "stale" || rd.StoreAsOfMs <= 0 {
		t.Fatalf("bucket down, cached heads serve: %d %+v", code, rd)
	}

	bucket.down.Store(false)
	if code, rd := readyz(t, s); code != 200 || rd.Status != "ready" {
		t.Fatalf("bucket back: %d %+v", code, rd)
	}
}

func TestReadinessWithNothingCachedIsUnavailable(t *testing.T) {
	bucket := &flakyBucket{Backend: store.NewMemory()}
	bucket.down.Store(true)
	// A process whose bucket never answered holds no head to serve.
	s := &server{store: store.NewServing(bucket)}
	if code, rd := readyz(t, s); code != 503 || rd.Status != "unavailable" || rd.Error == "" {
		t.Fatalf("nothing cached: %d %+v", code, rd)
	}
}

func TestShutdownDrainsThenFinishesRequestsInFlight(t *testing.T) {
	s := &server{}
	started := make(chan struct{})
	mux := http.NewServeMux()
	mux.HandleFunc("/readyz", s.handleReadyz)
	mux.HandleFunc("/slow", func(w http.ResponseWriter, r *http.Request) {
		close(started)
		time.Sleep(300 * time.Millisecond)
		io.WriteString(w, "done")
	})
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	base := "http://" + ln.Addr().String()
	served := make(chan error, 1)
	go func() { served <- s.serve(ln, newHTTPServer(mux), 200*time.Millisecond) }()

	slow := make(chan string, 1)
	go func() {
		resp, err := http.Get(base + "/slow")
		if err != nil {
			slow <- "error: " + err.Error()
			return
		}
		b, _ := io.ReadAll(resp.Body)
		resp.Body.Close()
		slow <- string(b)
	}()
	<-started
	syscall.Kill(syscall.Getpid(), syscall.SIGTERM)

	// During the drain the listener still answers, and readiness is 503.
	deadline := time.Now().Add(150 * time.Millisecond)
	for !s.draining.Load() && time.Now().Before(deadline) {
		time.Sleep(5 * time.Millisecond)
	}
	resp, err := http.Get(base + "/readyz")
	if err != nil {
		t.Fatalf("readyz while draining: %v", err)
	}
	resp.Body.Close()
	if resp.StatusCode != 503 {
		t.Fatalf("readyz while draining: %d", resp.StatusCode)
	}

	if got := <-slow; got != "done" {
		t.Fatalf("the request in flight finishes: %q", got)
	}
	select {
	case err := <-served:
		if err != nil {
			t.Fatalf("serve: %v", err)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("serve did not return after shutdown")
	}
}
