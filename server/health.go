package main

// /healthz reads nothing, so a probe never restarts a process for a
// bucket outage. Restarting cannot fix the bucket.
//
// /readyz answers 200 "degraded" while the bucket does not answer but
// the cached heads serve labelled reads (D-224). Every instance
// shares the bucket, so taking them all out of rotation would turn a
// write outage into a read outage. It answers 503 "unavailable" when
// the store has no cached head, and 503 "draining" once shutdown has
// begun, so the balancer stops sending before the listener closes.
//
// The probe reads the edits pointer's head, which the store rechecks at
// most once per FreshnessWindow.

import (
	"encoding/json"
	"net/http"
)

type readiness struct {
	Status string `json:"status"` // ready | degraded | unavailable | draining
	// Store is "none" with no -store: the dev store holds nothing durable.
	Store       string `json:"store"`
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

func (s *server) storeStatus() string {
	if s.store == nil {
		return "none"
	}
	return "ok"
}
