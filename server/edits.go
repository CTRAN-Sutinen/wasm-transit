package main

// Edit sync: GET/POST /edits. The host treats documents as opaque
// JSON — merge, the submit transition, and validation all happen in
// the wasm artifact via the sync_edits op, so client and server agree
// by construction (same bytes, same input, same output).
//
// POST /edits  body: the client's document ([]EditRecord).
//              response: { records: merged document, reports: [...] }
//              — the client adopts `records` wholesale.
// GET  /edits  returns the server's document.

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"time"
)

func (s *server) handleEdits(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		s.handleGetEdits(w, r)
	case http.MethodPost:
		s.handlePostEdits(w, r)
	default:
		http.Error(w, "use GET or POST", http.StatusMethodNotAllowed)
	}
}

func (s *server) handleGetEdits(w http.ResponseWriter, r *http.Request) {
	s.editsMu.Lock()
	defer s.editsMu.Unlock()
	doc := s.edits
	if len(doc) == 0 {
		doc = json.RawMessage("[]")
	}
	w.Header().Set("Content-Type", "application/json")
	w.Write(doc)
}

func (s *server) handlePostEdits(w http.ResponseWriter, r *http.Request) {
	body, err := io.ReadAll(io.LimitReader(r.Body, 32<<20))
	if err != nil {
		http.Error(w, "read body: "+err.Error(), http.StatusBadRequest)
		return
	}

	// The feed the merged document is validated against, when one is
	// configured (nil skips reports).
	var feedID *uint32
	if s.feeder != nil {
		feed, err := s.feeder.Load(r.Context(), s.wasm)
		if err != nil {
			http.Error(w, "feed unavailable: "+err.Error(), http.StatusInternalServerError)
			return
		}
		feedID = &feed.feedID
	}

	s.editsMu.Lock()
	defer s.editsMu.Unlock()

	serverDoc := s.edits
	if len(serverDoc) == 0 {
		serverDoc = json.RawMessage("[]")
	}

	// One trip into the artifact: merge + submit + validate. The host
	// passes the clock; the artifact owns every rule.
	var resp struct {
		Records json.RawMessage `json:"records"`
		Reports json.RawMessage `json:"reports"`
	}
	err = s.wasm.Invoke(r.Context(), map[string]any{
		"op":      "sync_edits",
		"feed_id": feedID,
		"client":  json.RawMessage(body),
		"server":  serverDoc,
		"now_ms":  time.Now().UnixMilli(),
	}, &resp)
	if err != nil {
		http.Error(w, "sync op failed: "+err.Error(), http.StatusInternalServerError)
		return
	}

	if err := s.persistEdits(resp.Records); err != nil {
		http.Error(w, "persist edits: "+err.Error(), http.StatusInternalServerError)
		return
	}
	s.edits = resp.Records

	w.Header().Set("Content-Type", "application/json")
	w.Write(resp.Records)
}

// persistEdits writes the document atomically (temp + rename) when a
// path is configured; in-memory mode is a no-op.
func (s *server) persistEdits(doc json.RawMessage) error {
	if s.editsPath == "" {
		return nil
	}
	tmp := s.editsPath + ".tmp"
	if err := os.WriteFile(tmp, doc, 0o644); err != nil {
		return fmt.Errorf("write %s: %w", tmp, err)
	}
	return os.Rename(tmp, s.editsPath)
}

// loadEdits restores a persisted document at startup.
func loadEdits(path string) json.RawMessage {
	if path == "" {
		return nil
	}
	body, err := os.ReadFile(path)
	if err != nil {
		if !os.IsNotExist(err) {
			fmt.Fprintf(os.Stderr, "warning: read %s: %v\n", path, err)
		}
		return nil
	}
	return json.RawMessage(body)
}
