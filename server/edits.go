package main

// Edit sync: GET/POST /edits. The host treats documents as opaque
// JSON — the merge and the submit transition happen in the SYNC
// CAPABILITY (cap-sync) via its one op, so client and server agree
// by construction (same bytes, same input, same output).
//
// POST /edits  body: the client's edit document (an older version —
//              a bare array — comes forward through cap-migrate).
//              response: the converged document — the client
//              adopts it wholesale.
// GET  /edits  returns the server's document
//              ({"kind":"edit_records","schema_version":2,"records":[…]}).
//
// With -store, /edits reads and writes the disruptions log
// (disruptions.go; docs/FORMATS.md, "The logs"): POST writes one entry
// through the merge, GET serves the fold (ETag = its hash). Without it,
// the dev store keeps it in memory or the -edits file.

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"strconv"
	"strings"
	"time"

	"rustwasm/server/store"
)

func (s *server) handleEdits(w http.ResponseWriter, r *http.Request) {
	if s.store != nil {
		s.handleStoreEdits(w, r)
		return
	}
	switch r.Method {
	case http.MethodGet:
		s.handleGetEdits(w, r)
	case http.MethodPost:
		s.handlePostEdits(w, r)
	default:
		http.Error(w, "use GET or POST", http.StatusMethodNotAllowed)
	}
}

// handleStoreEdits is /edits over the store: the disruptions log is the
// authority (disruptions.go). GET serves its fold, and a POST writes one
// entry through the receive path, retrying a lost race (a contending
// writer is read again and merged again, never clobbered).
func (s *server) handleStoreEdits(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		document, head, err := s.disruptionsNow()
		if err != nil {
			fail(w, err, http.StatusInternalServerError)
			return
		}
		labelStale(w, head)
		etag := `"` + head.Hash + `"`
		w.Header().Set("ETag", etag)
		w.Header().Set("Cache-Control", "no-cache")
		if r.Header.Get("If-None-Match") == etag {
			w.WriteHeader(http.StatusNotModified)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		w.Write(document)
	case http.MethodPost:
		body, err := io.ReadAll(io.LimitReader(r.Body, 32<<20))
		if err != nil {
			http.Error(w, "read body: "+err.Error(), http.StatusBadRequest)
			return
		}
		writer := writerOf(r, "anonymous")
		document, hash, _, unknown, err := s.commitDisruptions(json.RawMessage(body), writer, nil)
		if err != nil {
			editsWriteFailed(w, err)
			return
		}
		flagUnknown(w, "edits", writer, unknown)
		w.Header().Set("Content-Type", "application/json")
		w.Header().Set("ETag", `"`+hash+`"`)
		w.Write([]byte(document))
	default:
		http.Error(w, "use GET or POST", http.StatusMethodNotAllowed)
	}
}

// editsWriteFailed answers a write the disruptions log did not take: a
// bucket that does not answer or a gap is 503 (nothing was written;
// resend), a lost race on every attempt is 409, and a refused document
// is 400.
func editsWriteFailed(w http.ResponseWriter, err error) {
	var ge *store.GapError
	var ce *store.ConflictError
	switch {
	case errors.Is(err, store.ErrUnavailable):
		fail(w, err, http.StatusServiceUnavailable)
	case errors.As(err, &ge):
		w.Header().Set("Retry-After", strconv.Itoa(retryAfterSecs))
		http.Error(w, err.Error(), http.StatusServiceUnavailable)
	case errors.As(err, &ce):
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusConflict)
		json.NewEncoder(w).Encode(map[string]any{"current": ce.Current, "seq": ce.Seq})
	case strings.Contains(err.Error(), "moved on every attempt"):
		http.Error(w, "conflict: the disruptions log moved, retry", http.StatusConflict)
	default:
		fail(w, err, http.StatusBadRequest)
	}
}

// maybeCompactEdits runs the D-57 tombstone-retention write at startup:
// cap-sync's compact_edits drops the retired records older than the
// horizon from the fold, and the drop is one entry in the disruptions
// log (writer gc), audited like any other write. The entries before it
// keep every record.
func (s *server) maybeCompactEdits() {
	if s.store == nil {
		return
	}
	nowMs := time.Now().UnixMilli()
	horizonMs := int64(s.retentionDays) * 24 * 3600 * 1000
	dropped := 0
	seq, _, _, err := s.writeLog(s.disruptionsFold(), logWrite{writer: "gc", op: "compact_edits"}, func() ([]json.RawMessage, *logRefusal, error) {
		records, _, _, _, err := s.readEdits(s.disruptionsDoc.doc)
		if err != nil {
			return nil, nil, err
		}
		var resp struct {
			Document string `json:"document"`
			Dropped  int    `json:"dropped"`
		}
		if err := s.syncOp(map[string]any{
			"op": "compact_edits", "records": records, "now_ms": nowMs, "horizon_ms": horizonMs,
		}, &resp); err != nil || resp.Dropped == 0 {
			return nil, nil, err
		}
		dropped = resp.Dropped
		next, err := s.editRecords(resp.Document)
		return next, nil, err
	})
	if err == nil && dropped > 0 {
		fmt.Printf("compacted %d tombstone(s) past the %d-day horizon (disruptions entry %d)\n", dropped, s.retentionDays, seq)
	}
}

func (s *server) handleGetEdits(w http.ResponseWriter, r *http.Request) {
	s.editsMu.Lock()
	defer s.editsMu.Unlock()
	doc := s.edits
	if len(doc) == 0 {
		doc = json.RawMessage(emptyEdits)
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

	s.editsMu.Lock()
	defer s.editsMu.Unlock()

	serverDoc := s.edits
	if len(serverDoc) == 0 {
		serverDoc = json.RawMessage(emptyEdits)
	}

	// One trip into the sync capability: merge + submit, after both
	// documents are read at the receive boundary (an older version
	// brought forward). The host passes the clock; the module owns
	// every rule. Validation is NOT here — what a record amounts to on
	// the network is cap-divert's reports, asked through the bus (a
	// record the network cannot answer is stored all the same, and
	// patches nothing).
	document, _, unknown, err := s.mergeEdits(json.RawMessage(body), serverDoc, time.Now().UnixMilli())
	if err != nil {
		http.Error(w, "sync op failed: "+err.Error(), http.StatusBadRequest)
		return
	}
	flagUnknown(w, "edits", writerOf(r, "anonymous"), unknown)

	if err := s.persistEdits(json.RawMessage(document)); err != nil {
		http.Error(w, "persist edits: "+err.Error(), http.StatusInternalServerError)
		return
	}
	s.edits = json.RawMessage(document)

	w.Header().Set("Content-Type", "application/json")
	w.Write([]byte(document))
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
