package main

// Health and readiness for a load balancer or an orchestrator.
//
//   - /healthz (liveness): 200 while the process answers HTTP. It reads
//     nothing, so a probe never restarts a process for a bucket's
//     outage — restarting cannot fix the bucket.
//   - /readyz (readiness): whether this process should be sent traffic.
//     200 "ready"; 200 "degraded" while the bucket does not answer but
//     the cached heads serve labelled reads (D-119) — every instance
//     shares the bucket, so taking them all out of rotation would turn
//     a write outage into a read outage; 503 "unavailable" when the
//     store has no cached head to serve; 503 "draining" once shutdown
//     has begun, so the balancer stops sending before the listener
//     closes.
//
// The probe reads the edits pointer's head, which the store caches and
// rechecks at most once per FreshnessWindow, so a probe every second
// costs the bucket at most one Stat every two seconds.

import (
	"encoding/json"
	"net/http"
)

// readiness is the /readyz body.
type readiness struct {
	Status string `json:"status"` // ready | degraded | unavailable | draining
	// Store is "ok", "stale", "unavailable", or "none" (no -store: the
	// dev store, nothing durable).
	Store string `json:"store"`
	// StoreAsOfMs is when the bucket last confirmed the head (Unix ms),
	// while Store is "stale".
	StoreAsOfMs int64  `json:"store_as_of_ms,omitempty"`
	Error       string `json:"error,omitempty"`
}

func (s *server) handleHealthz(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Cache-Control", "no-store")
	w.Header().Set("Content-Type", "text/plain; charset=utf-8")
	w.Write([]byte("ok\n"))
}

func (s *server) handleReadyz(w http.ResponseWriter, r *http.Request) {
	rd := s.readiness()
	code := http.StatusOK
	if rd.Status == "unavailable" || rd.Status == "draining" {
		code = http.StatusServiceUnavailable
	}
	w.Header().Set("Cache-Control", "no-store")
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(code)
	json.NewEncoder(w).Encode(rd)
}

func (s *server) readiness() readiness {
	if s.draining.Load() {
		return readiness{Status: "draining", Store: s.storeStatus()}
	}
	if s.store == nil {
		return readiness{Status: "ready", Store: "none"}
	}
	head, _, err := s.store.Head("edits")
	switch {
	case err != nil:
		return readiness{Status: "unavailable", Store: "unavailable", Error: err.Error()}
	case head.Stale:
		return readiness{Status: "degraded", Store: "stale", StoreAsOfMs: head.AsOfMs}
	}
	return readiness{Status: "ready", Store: "ok"}
}

// storeStatus names the store without reading it (for draining).
func (s *server) storeStatus() string {
	if s.store == nil {
		return "none"
	}
	return "ok"
}
