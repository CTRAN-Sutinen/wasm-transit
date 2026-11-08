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
// With -store, /edits is the edit_records pointer of the content-
// addressed store (docs/FORMATS.md): POST runs the CAS retry loop over
// the merge, GET serves the head blob (ETag = its hash). Without it,
// the legacy flat-file (-edits) or in-memory path runs unchanged.

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
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

// handleStoreEdits is /edits over the store: the pointer's head blob is
// the authority, and a POST commits the merged document through a CAS
// retry loop (a contending writer is re-read and re-merged, never
// clobbered).
func (s *server) handleStoreEdits(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		head, found, err := s.store.Head("edits")
		if err != nil {
			fail(w, err, http.StatusInternalServerError)
			return
		}
		if !found {
			w.Header().Set("Content-Type", "application/json")
			w.Write([]byte(emptyEdits))
			return
		}
		data, _, err := s.store.GetBlob(head.Hash)
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
		w.Write(data)
	case http.MethodPost:
		body, err := io.ReadAll(io.LimitReader(r.Body, 32<<20))
		if err != nil {
			http.Error(w, "read body: "+err.Error(), http.StatusBadRequest)
			return
		}
		document, hash, err := s.commitEdits(json.RawMessage(body), writerOf(r, "anonymous"))
		if err != nil {
			if errors.Is(err, store.ErrUnavailable) {
				fail(w, err, http.StatusServiceUnavailable)
				return
			}
			if strings.Contains(err.Error(), "moved") {
				http.Error(w, "conflict: the edit document moved, retry", http.StatusConflict)
				return
			}
			fail(w, err, http.StatusBadRequest)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		w.Header().Set("ETag", `"`+hash+`"`)
		w.Write([]byte(document))
	default:
		http.Error(w, "use GET or POST", http.StatusMethodNotAllowed)
	}
}

// maybeCompactEdits runs the D-57 tombstone-retention commit at startup
// (and is reusable after): retired records older than the horizon are
// dropped by cap-sync's compact_edits, committed as an ordinary CAS'd
// log entry, so the compaction is audited like any other commit.
func (s *server) maybeCompactEdits() {
	if s.store == nil {
		return
	}
	nowMs := time.Now().UnixMilli()
	horizonMs := int64(s.retentionDays) * 24 * 3600 * 1000
	head, found, err := s.store.Head("edits")
	if err != nil || !found {
		return
	}
	body, _, err := s.store.GetBlob(head.Hash)
	if err != nil {
		return
	}
	records, _, _, _, err := s.readEdits(body)
	if err != nil {
		return
	}
	var resp struct {
		Document     string `json:"document"`
		DocumentHash string `json:"document_hash"`
		Dropped      int    `json:"dropped"`
	}
	if err := s.syncOp(map[string]any{
		"op": "compact_edits", "records": records, "now_ms": nowMs, "horizon_ms": horizonMs,
	}, &resp); err != nil || resp.Dropped == 0 {
		return
	}
	if _, _, err := s.store.CAS("edits", head.Hash, []byte(resp.Document), s.syncEnvelope(), resp.DocumentHash, "gc", nowMs); err == nil {
		fmt.Printf("compacted %d tombstone(s) past the %d-day horizon\n", resp.Dropped, s.retentionDays)
	}
}

// commitForward moves the edits pointer's head to the version the
// hosted modules read, at startup: an ordinary logged CAS whose writer
// names the migration module (migrate:<module hash>), the way the D-57
// compaction commits. The older blob stays readable under its hash.
func (s *server) commitForward() {
	if s.store == nil {
		return
	}
	head, found, err := s.store.Head("edits")
	if err != nil || !found {
		return
	}
	body, _, err := s.store.GetBlob(head.Hash)
	if err != nil {
		log.Printf("commit forward: edits head %s: %v", head.Hash, err)
		return
	}
	doc, migrated, err := s.forward("edit_records", body)
	if err != nil || !migrated {
		if err != nil {
			log.Printf("commit forward: edits: %v", err)
		}
		return
	}
	_, document, hash, _, err := s.readEdits(doc)
	if err != nil {
		log.Printf("commit forward: edits: %v", err)
		return
	}
	m, _ := s.migrator()
	if _, _, err := s.store.CAS("edits", head.Hash, []byte(document), s.syncEnvelope(), hash, "migrate:"+m.Hash(), time.Now().UnixMilli()); err != nil {
		log.Printf("commit forward: edits: %v", err)
		return
	}
	log.Printf("commit forward: edits %s -> %s (edit_records v%d)", head.Hash, hash, editsVersion)
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
	document, _, err := s.mergeEdits(json.RawMessage(body), serverDoc, time.Now().UnixMilli())
	if err != nil {
		http.Error(w, "sync op failed: "+err.Error(), http.StatusBadRequest)
		return
	}

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
