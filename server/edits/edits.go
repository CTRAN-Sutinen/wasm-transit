// Package edits is the edit document's protocol: GET and POST /edits.
// The host treats documents as opaque JSON, and cap-sync does the merge,
// so client and server agree. A bare array from
// an older client comes forward through cap-migrate.
//
// With -store, /edits reads and writes the disruptions log (server/logs;
// docs/FORMATS.md, "The logs"). Without it, the dev store keeps the
// document in memory or in the -edits file.
package edits

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"strconv"
	"sync"
	"time"

	"rustwasm/server/httpx"
	"rustwasm/server/logs"
	"rustwasm/server/store"
	"rustwasm/server/wasmhost"
)

// Deps is what the host gives the edit document.
type Deps struct {
	Store func() *store.Store
	Logs  *logs.Logs
	// Sync speaks as cap-sync.
	Sync    func() *wasmhost.Host
	Forward func(kind string, doc json.RawMessage) (json.RawMessage, bool, error)
	// Path is the dev store's file (-edits); "" keeps the document in
	// memory only.
	Path string
	// RetentionDays is the tombstones' horizon (D-57).
	RetentionDays int
}

type Edits struct {
	store         func() *store.Store
	logs          *logs.Logs
	sync          func() *wasmhost.Host
	forward       func(kind string, doc json.RawMessage) (json.RawMessage, bool, error)
	path          string
	retentionDays int

	// mu guards doc, the dev store's document.
	mu  sync.Mutex
	doc json.RawMessage
}

// New reads the dev store's file, if any.
func New(d Deps) *Edits {
	return &Edits{store: d.Store, logs: d.Logs, sync: d.Sync, forward: d.Forward, path: d.Path, retentionDays: d.RetentionDays, doc: load(d.Path)}
}

// The disruptions log (D-190; docs/FORMATS.md, "The logs"). In store mode
// the edit document is the fold of the log's entries, which cap-sync's
// `receive` writes. The old `edits` pointer stays in the bucket, read only.
const DisruptionsLog = "disruptions"

// While the bucket does not answer, it answers the fold this process
// holds, stale.
func (e *Edits) Now() (document []byte, head store.Head, err error) {
	l := e.logs.Get(DisruptionsLog)
	if l == nil {
		return nil, store.Head{Name: DisruptionsLog}, logs.ErrNoLog(DisruptionsLog)
	}
	document, head, err = e.logs.Current(l, "edit_records")
	if err == nil {
		head.Envelope = e.Envelope()
	}
	return document, head, err
}

func (e *Edits) HandleEdits(w http.ResponseWriter, r *http.Request) {
	if e.store() != nil {
		e.handleStoreEdits(w, r)
		return
	}
	switch r.Method {
	case http.MethodGet:
		e.handleGetEdits(w, r)
	case http.MethodPost:
		e.handlePostEdits(w, r)
	default:
		http.Error(w, "use GET or POST", http.StatusMethodNotAllowed)
	}
}

// handleStoreEdits retries a lost race, so a contending writer is read
// again and merged again, never clobbered.
func (e *Edits) handleStoreEdits(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		document, head, err := e.Now()
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
		_, got, refusal, err := e.Write(r, json.RawMessage(body), nil)
		if err != nil {
			WriteFailed(w, err)
			return
		}
		if refusal != nil {
			Refused(w, got, refusal)
			return
		}
		httpx.FlagUnknown(w, "edits", writer, got.UnknownFields)
		document, head, err := e.Now()
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

// Write clamps the document's clock to the entry's. A non-nil from
// is the hash that the writer last saw, and a head that moved is refused.
// The route checked the write rule (`edits`).
func (e *Edits) Write(r *http.Request, client json.RawMessage, from *string) (int64, logs.Received, *logs.Refusal, error) {
	l := e.logs.Get(DisruptionsLog)
	if l == nil {
		return 0, logs.Received{}, nil, logs.ErrNoLog(DisruptionsLog)
	}
	args := map[string]any{"client": client}
	if from != nil {
		args["from"] = *from
	}
	op, _ := json.Marshal(map[string]any{"cap": "sync", "op": "sync", "args": args})
	seq, _, got, refusal, err := e.logs.Receive(r.Context(), nil, l, logs.Write{Writer: httpx.Writer(r, "anonymous"), Op: "POST /edits", ArgsHash: store.HashOf(client)}, logs.Call{Ops: []json.RawMessage{op}})
	return seq, got, refusal, err
}

func Refused(w http.ResponseWriter, got logs.Received, refusal *logs.Refusal) {
	if got.Failed != nil {
		httpx.AnswerFailed(w, got.Failed.Error, http.StatusBadRequest, "")
		return
	}
	logs.WriteRefused(w, refusal)
}

func WriteFailed(w http.ResponseWriter, err error) {
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

// Compact runs the D-57 tombstone retention at startup. The
// drop is one entry (writer gc), and the entries before it keep every
// record.
func (e *Edits) Compact() {
	l := e.logs.Get(DisruptionsLog)
	if e.store() == nil || l == nil {
		return
	}
	horizonMs := int64(e.retentionDays) * 24 * 3600 * 1000
	op, _ := json.Marshal(map[string]any{"cap": "sync", "op": "compact", "args": map[string]any{"horizon_ms": horizonMs}})
	before := l.Seq()
	seq, _, got, refusal, err := e.logs.Receive(context.Background(), nil, l, logs.Write{Writer: "gc", Op: "compact_edits"}, logs.Call{Ops: []json.RawMessage{op}})
	if err == nil && refusal == nil && seq > before && got.Entry != "" {
		fmt.Printf("compacted the tombstones past the %d-day horizon (disruptions entry %d, %d change(s))\n", e.retentionDays, seq, got.Changes)
	}
}

func (e *Edits) handleGetEdits(w http.ResponseWriter, r *http.Request) {
	e.mu.Lock()
	defer e.mu.Unlock()
	doc := e.doc
	if len(doc) == 0 {
		doc = json.RawMessage(emptyEdits)
	}
	w.Header().Set("Content-Type", "application/json")
	w.Write(doc)
}

func (e *Edits) handlePostEdits(w http.ResponseWriter, r *http.Request) {
	body, err := io.ReadAll(io.LimitReader(r.Body, 32<<20))
	if err != nil {
		http.Error(w, "read body: "+err.Error(), http.StatusBadRequest)
		return
	}

	e.mu.Lock()
	defer e.mu.Unlock()

	serverDoc := e.doc
	if len(serverDoc) == 0 {
		serverDoc = json.RawMessage(emptyEdits)
	}

	// The host passes the clock and cap-sync owns every rule. It does not
	// validate: cap-divert reports what a record amounts to, and a record
	// that the network cannot answer is stored and patches nothing.
	document, _, unknown, err := e.mergeEdits(json.RawMessage(body), serverDoc, time.Now().UnixMilli())
	if err != nil {
		http.Error(w, "sync op failed: "+err.Error(), http.StatusBadRequest)
		return
	}
	httpx.FlagUnknown(w, "edits", httpx.Writer(r, "anonymous"), unknown)

	if err := e.persistEdits(json.RawMessage(document)); err != nil {
		http.Error(w, "persist edits: "+err.Error(), http.StatusInternalServerError)
		return
	}
	e.doc = json.RawMessage(document)

	w.Header().Set("Content-Type", "application/json")
	w.Write([]byte(document))
}

// persistEdits is a no-op in memory mode.
func (e *Edits) persistEdits(doc json.RawMessage) error {
	if e.path == "" {
		return nil
	}
	tmp := e.path + ".tmp"
	if err := os.WriteFile(tmp, doc, 0o644); err != nil {
		return fmt.Errorf("write %s: %w", tmp, err)
	}
	return os.Rename(tmp, e.path)
}

func load(path string) json.RawMessage {
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

// Version mirrors transit_sync::SCHEMA_VERSION. An older document
// comes forward through cap-migrate at receive.
const Version = 2

const emptyEdits = `{"kind":"edit_records","schema_version":2,"records":[]}`

// The conflict function is content-addressed with the producing module's
// hash, so the envelope carries it.
func (e *Edits) Envelope() store.Envelope {
	return store.Envelope{
		Kind:          "edit_records",
		SchemaVersion: Version,
		ProducedBy:    "cap-sync@" + e.sync().Hash(),
	}
}

func (e *Edits) syncOp(req map[string]any, out any) error {
	return e.sync().Invoke(context.Background(), req, out)
}

func (e *Edits) readEdits(raw json.RawMessage) (records json.RawMessage, document string, hash string, dropped int, err error) {
	if raw, _, err = e.forward("edit_records", raw); err != nil {
		return nil, "", "", 0, err
	}
	var resp struct {
		Records       json.RawMessage `json:"records"`
		Dropped       int             `json:"dropped"`
		SchemaVersion uint32          `json:"schema_version"`
		Document      string          `json:"document"`
		DocumentHash  string          `json:"document_hash"`
	}
	if err := e.syncOp(map[string]any{"op": "read_edits", "document": raw}, &resp); err != nil {
		return nil, "", "", 0, err
	}
	return resp.Records, resp.Document, resp.DocumentHash, resp.Dropped, nil
}

// mergeEdits clamps the client's clock to the server's (D-56). The answer
// carries the fields that the client's records have and this build lacks.
func (e *Edits) mergeEdits(clientRaw, serverRaw json.RawMessage, nowMs int64) (document string, hash string, unknown []string, err error) {
	clientRec, _, _, _, err := e.readEdits(clientRaw)
	if err != nil {
		return "", "", nil, fmt.Errorf("read_edits client: %w", err)
	}
	serverRec, _, _, _, err := e.readEdits(serverRaw)
	if err != nil {
		return "", "", nil, fmt.Errorf("read_edits server: %w", err)
	}
	var resp struct {
		Document      string   `json:"document"`
		DocumentHash  string   `json:"document_hash"`
		UnknownFields []string `json:"unknown_fields"`
	}
	if err := e.syncOp(map[string]any{
		"op": "sync_edits", "client": clientRec, "server": serverRec, "now_ms": nowMs,
	}, &resp); err != nil {
		return "", "", nil, fmt.Errorf("sync_edits: %w", err)
	}
	return resp.Document, resp.DocumentHash, resp.UnknownFields, nil
}
