package main

// GET and POST /edits. The host treats documents as opaque JSON, and
// cap-sync does the merge, so client and server agree. A bare array from
// an older client comes forward through cap-migrate.
//
// With -store, /edits reads and writes the disruptions log (server/logs;
// docs/FORMATS.md, "The logs"). Without it, the dev store keeps the
// document in memory or in the -edits file.

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"strconv"
	"time"

	"rustwasm/server/httpx"
	"rustwasm/server/logs"
	"rustwasm/server/store"
)

// The disruptions log (D-190; docs/FORMATS.md, "The logs"). In store mode
// the edit document is the fold of the log's entries, which cap-sync's
// `receive` writes. The old `edits` pointer stays in the bucket, read only.
const disruptionsLog = "disruptions"

// While the bucket does not answer, it answers the fold this process
// holds, stale.
func (s *server) disruptionsNow() (document []byte, head store.Head, err error) {
	l := s.kept().Get(disruptionsLog)
	if l == nil {
		return nil, store.Head{Name: disruptionsLog}, logs.ErrNoLog(disruptionsLog)
	}
	document, head, err = s.kept().Current(l, "edit_records")
	if err == nil {
		head.Envelope = s.syncEnvelope()
	}
	return document, head, err
}

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

// handleStoreEdits retries a lost race, so a contending writer is read
// again and merged again, never clobbered.
func (s *server) handleStoreEdits(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		document, head, err := s.disruptionsNow()
		if err != nil {
			httpx.Fail(w, err, http.StatusInternalServerError)
			return
		}
		httpx.LabelStale(w, head)
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
		writer := httpx.Writer(r, "anonymous")
		_, got, refusal, err := s.writeEdits(r, json.RawMessage(body), nil)
		if err != nil {
			editsWriteFailed(w, err)
			return
		}
		if refusal != nil {
			editsRefused(w, got, refusal)
			return
		}
		httpx.FlagUnknown(w, "edits", writer, got.UnknownFields)
		document, head, err := s.disruptionsNow()
		if err != nil {
			httpx.Fail(w, err, http.StatusInternalServerError)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		w.Header().Set("ETag", `"`+head.Hash+`"`)
		w.Write(document)
	default:
		http.Error(w, "use GET or POST", http.StatusMethodNotAllowed)
	}
}

// writeEdits clamps the document's clock to the entry's. A non-nil from
// is the hash that the writer last saw, and a head that moved is refused.
// The route checked the write rule (`edits`).
func (s *server) writeEdits(r *http.Request, client json.RawMessage, from *string) (int64, logs.Received, *logs.Refusal, error) {
	l := s.kept().Get(disruptionsLog)
	if l == nil {
		return 0, logs.Received{}, nil, logs.ErrNoLog(disruptionsLog)
	}
	args := map[string]any{"client": client}
	if from != nil {
		args["from"] = *from
	}
	op, _ := json.Marshal(map[string]any{"cap": "sync", "op": "sync", "args": args})
	seq, _, got, refusal, err := s.kept().Receive(r.Context(), nil, l, logs.Write{Writer: httpx.Writer(r, "anonymous"), Op: "POST /edits", ArgsHash: store.HashOf(client)}, logs.Call{Ops: []json.RawMessage{op}})
	return seq, got, refusal, err
}

func editsRefused(w http.ResponseWriter, got logs.Received, refusal *logs.Refusal) {
	if got.Failed != nil {
		httpx.AnswerFailed(w, got.Failed.Error, http.StatusBadRequest, "")
		return
	}
	logs.WriteRefused(w, refusal)
}

func editsWriteFailed(w http.ResponseWriter, err error) {
	var ge *store.GapError
	var ce *store.ConflictError
	switch {
	case errors.Is(err, store.ErrUnavailable):
		httpx.Fail(w, err, http.StatusServiceUnavailable)
	case errors.As(err, &ge):
		w.Header().Set("Retry-After", strconv.Itoa(httpx.RetryAfterSecs))
		http.Error(w, err.Error(), http.StatusServiceUnavailable)
	case errors.As(err, &ce):
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusConflict)
		json.NewEncoder(w).Encode(map[string]any{"current": ce.Current, "seq": ce.Seq})
	case errors.Is(err, logs.ErrMoved):
		http.Error(w, "conflict: the disruptions log moved, retry", http.StatusConflict)
	default:
		httpx.Fail(w, err, http.StatusBadRequest)
	}
}

// maybeCompactEdits runs the D-57 tombstone retention at startup. The
// drop is one entry (writer gc), and the entries before it keep every
// record.
func (s *server) maybeCompactEdits() {
	l := s.kept().Get(disruptionsLog)
	if s.store == nil || l == nil {
		return
	}
	horizonMs := int64(s.retentionDays) * 24 * 3600 * 1000
	op, _ := json.Marshal(map[string]any{"cap": "sync", "op": "compact", "args": map[string]any{"horizon_ms": horizonMs}})
	before := l.Seq()
	seq, _, got, refusal, err := s.kept().Receive(context.Background(), nil, l, logs.Write{Writer: "gc", Op: "compact_edits"}, logs.Call{Ops: []json.RawMessage{op}})
	if err == nil && refusal == nil && seq > before && got.Entry != "" {
		fmt.Printf("compacted the tombstones past the %d-day horizon (disruptions entry %d, %d change(s))\n", s.retentionDays, seq, got.Changes)
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

	// The host passes the clock and cap-sync owns every rule. It does not
	// validate: cap-divert reports what a record amounts to, and a record
	// that the network cannot answer is stored and patches nothing.
	document, _, unknown, err := s.mergeEdits(json.RawMessage(body), serverDoc, time.Now().UnixMilli())
	if err != nil {
		http.Error(w, "sync op failed: "+err.Error(), http.StatusBadRequest)
		return
	}
	httpx.FlagUnknown(w, "edits", httpx.Writer(r, "anonymous"), unknown)

	if err := s.persistEdits(json.RawMessage(document)); err != nil {
		http.Error(w, "persist edits: "+err.Error(), http.StatusInternalServerError)
		return
	}
	s.edits = json.RawMessage(document)

	w.Header().Set("Content-Type", "application/json")
	w.Write([]byte(document))
}

// persistEdits is a no-op in memory mode.
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
