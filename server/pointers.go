package main

// The pointer and blob routes over the content-addressed store
// (docs/architecture/storage.md; routes in openapi.json).
//
// /blobs also answers the published documents (publishedBlob) with or
// without the store. The edits pointer's log is the pointer's history
// before the disruptions log, read only. The published plan set, the
// picks' networks, and the derived documents are refused here: they
// publish through POST /plans and derived.go.

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"strings"
	"time"

	"rustwasm/server/store"
)

// editsVersion and networkVersion mirror transit_sync::SCHEMA_VERSION and
// transit_network::SCHEMA_VERSION. An older document comes forward
// through cap-migrate at receive.
const editsVersion = 2

const networkVersion = 2

const emptyEdits = `{"kind":"edit_records","schema_version":2,"records":[]}`

// The conflict function is content-addressed with the producing module's
// hash, so the envelope carries it.
func (s *server) syncEnvelope() store.Envelope {
	return store.Envelope{
		Kind:          "edit_records",
		SchemaVersion: editsVersion,
		ProducedBy:    "cap-sync@" + s.wasm.Hash(),
	}
}

func (s *server) migrator() (*WasmHost, error) {
	return s.as("migrate"), nil
}

// forward returns a current or newer document unchanged. The module that
// reads a newer one refuses it, so the host never rewrites it.
func (s *server) forward(kind string, raw json.RawMessage) (doc json.RawMessage, migrated bool, err error) {
	m, err := s.migrator()
	if err != nil {
		return nil, false, fmt.Errorf("migration module: %w", err)
	}
	var v struct {
		Version uint32 `json:"version"`
		Reads   uint32 `json:"reads"`
	}
	if err := m.Invoke(context.Background(), map[string]any{"op": "version", "kind": kind, "document": raw}, &v); err != nil {
		return nil, false, err
	}
	if v.Version >= v.Reads {
		return raw, false, nil
	}
	var out struct {
		Canonical string `json:"canonical"`
	}
	if err := m.Invoke(context.Background(), map[string]any{"op": "migrate", "kind": kind, "document": raw}, &out); err != nil {
		return nil, false, err
	}
	return json.RawMessage(out.Canonical), true, nil
}

func (s *server) syncOp(req map[string]any, out any) error {
	return s.wasm.Invoke(context.Background(), req, out)
}

func (s *server) readEdits(raw json.RawMessage) (records json.RawMessage, document string, hash string, dropped int, err error) {
	if raw, _, err = s.forward("edit_records", raw); err != nil {
		return nil, "", "", 0, err
	}
	var resp struct {
		Records       json.RawMessage `json:"records"`
		Dropped       int             `json:"dropped"`
		SchemaVersion uint32          `json:"schema_version"`
		Document      string          `json:"document"`
		DocumentHash  string          `json:"document_hash"`
	}
	if err := s.syncOp(map[string]any{"op": "read_edits", "document": raw}, &resp); err != nil {
		return nil, "", "", 0, err
	}
	return resp.Records, resp.Document, resp.DocumentHash, resp.Dropped, nil
}

// mergeEdits clamps the client's clock to the server's (D-56). The answer
// carries the fields that the client's records have and this build lacks.
func (s *server) mergeEdits(clientRaw, serverRaw json.RawMessage, nowMs int64) (document string, hash string, unknown []string, err error) {
	clientRec, _, _, _, err := s.readEdits(clientRaw)
	if err != nil {
		return "", "", nil, fmt.Errorf("read_edits client: %w", err)
	}
	serverRec, _, _, _, err := s.readEdits(serverRaw)
	if err != nil {
		return "", "", nil, fmt.Errorf("read_edits server: %w", err)
	}
	var resp struct {
		Document      string   `json:"document"`
		DocumentHash  string   `json:"document_hash"`
		UnknownFields []string `json:"unknown_fields"`
	}
	if err := s.syncOp(map[string]any{
		"op": "sync_edits", "client": clientRec, "server": serverRec, "now_ms": nowMs,
	}, &resp); err != nil {
		return "", "", nil, fmt.Errorf("sync_edits: %w", err)
	}
	return resp.Document, resp.DocumentHash, resp.UnknownFields, nil
}

func asConflict(err error, target **store.ConflictError) bool {
	for ; err != nil; err = errors.Unwrap(err) {
		if ce, ok := err.(*store.ConflictError); ok {
			*target = ce
			return true
		}
	}
	return false
}

func (s *server) handlePointers(w http.ResponseWriter, r *http.Request) {
	if s.store == nil {
		http.Error(w, "no store configured (start with -store <url>)", http.StatusNotFound)
		return
	}
	rest := strings.TrimPrefix(r.URL.Path, "/pointers/")
	parts := strings.Split(rest, "/")
	if len(parts) == 0 || parts[0] == "" {
		http.Error(w, "bad path", http.StatusBadRequest)
		return
	}
	name := parts[0]
	if len(parts) == 2 && parts[1] == "log" {
		if r.Method != http.MethodGet {
			http.Error(w, "use GET", http.StatusMethodNotAllowed)
			return
		}
		s.handlePointerLog(w, r, name)
		return
	}
	if len(parts) != 1 {
		http.Error(w, "bad path", http.StatusBadRequest)
		return
	}
	switch r.Method {
	case http.MethodGet:
		s.handlePointerHead(w, name)
	case http.MethodPost:
		s.handlePointerPost(w, r, name)
	default:
		http.Error(w, "use GET or POST", http.StatusMethodNotAllowed)
	}
}

func (s *server) handlePointerHead(w http.ResponseWriter, name string) {
	if name == "edits" {
		_, head, err := s.disruptionsNow()
		if err != nil {
			fail(w, err, http.StatusInternalServerError)
			return
		}
		head.Name = name
		labelStale(w, head)
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(head)
		return
	}
	head, found, err := s.store.Head(name)
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return
	}
	if !found {
		http.Error(w, "no such pointer", http.StatusNotFound)
		return
	}
	labelStale(w, head)
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(head)
}

func (s *server) handlePointerLog(w http.ResponseWriter, r *http.Request, name string) {
	since := r.URL.Query().Get("since")
	entries, found, err := s.store.Log(name, since)
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return
	}
	// Writer identities leave the store only for audit-granted callers.
	canAudit := true
	if v := r.Context().Value(auditKey); v != nil {
		canAudit, _ = v.(bool)
	}
	if !canAudit {
		for i := range entries {
			entries[i].Writer = ""
		}
	}
	head, _, _ := s.store.Head(name)
	labelStale(w, head)
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]any{
		"name":        name,
		"entries":     entries,
		"since_found": found,
		"current":     head.Hash,
		"whole":       !found, // a since outside the log: take the whole current document
	})
}

func (s *server) handlePointerPost(w http.ResponseWriter, r *http.Request, name string) {
	body, err := io.ReadAll(io.LimitReader(r.Body, 32<<20))
	if err != nil {
		http.Error(w, "read body: "+err.Error(), http.StatusBadRequest)
		return
	}
	var req struct {
		From         string          `json:"from"`
		Kind         string          `json:"kind"`          // required on create
		Client       json.RawMessage `json:"client"`        // edit_records: the client document
		Document     string          `json:"document"`      // other kinds: the raw document text
		DocumentHash string          `json:"document_hash"` // optional assertion
	}
	if err := json.Unmarshal(body, &req); err != nil {
		http.Error(w, "bad request: "+err.Error(), http.StatusBadRequest)
		return
	}
	writer := writerOf(r, "anonymous")
	nowMs := time.Now().UnixMilli()

	// One attempt on the client's last-seen hash. The caller retries on 409.
	if name == "edits" || req.Kind == "edit_records" {
		if name != "edits" {
			http.Error(w, "the edit document is the edits pointer, written as the disruptions log", http.StatusBadRequest)
			return
		}
		if len(req.Client) == 0 {
			http.Error(w, "edit_records takes a client document", http.StatusBadRequest)
			return
		}
		from := req.From
		seq, got, refusal, err := s.writeEdits(r, req.Client, &from)
		if err != nil {
			editsWriteFailed(w, err)
			return
		}
		if refusal != nil {
			editsRefused(w, got, refusal)
			return
		}
		flagUnknown(w, "edits", writer, got.UnknownFields)
		_, head, err := s.disruptionsNow()
		if err != nil {
			fail(w, err, http.StatusInternalServerError)
			return
		}
		hash := head.Hash
		w.Header().Set("Content-Type", "application/json")
		w.Header().Set("ETag", `"`+hash+`"`)
		json.NewEncoder(w).Encode(map[string]any{"name": name, "hash": hash, "seq": seq})
		return
	}

	head, found, err := s.store.Head(name)
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return
	}
	kind := req.Kind
	if found {
		kind = head.Envelope.Kind
	} else if kind == "" {
		http.Error(w, "kind is required when creating a pointer", http.StatusBadRequest)
		return
	}

	entry, refusal, err := s.pointerRefusal(name, kind)
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return
	}
	if refusal != "" {
		http.Error(w, refusal, http.StatusBadRequest)
		return
	}
	if req.Document == "" {
		http.Error(w, "document is required", http.StatusBadRequest)
		return
	}
	if req.DocumentHash != "" && req.DocumentHash != store.HashOf([]byte(req.Document)) {
		http.Error(w, fmt.Sprintf("hash mismatch: document_hash is %s, and the bytes are %s", req.DocumentHash, store.HashOf([]byte(req.Document))), http.StatusBadRequest)
		return
	}
	if !json.Valid([]byte(req.Document)) {
		http.Error(w, "document is not JSON", http.StatusBadRequest)
		return
	}
	checked, err := s.checkKind(r.Context(), entry, json.RawMessage(req.Document))
	if err != nil {
		http.Error(w, "the document is refused: "+err.Error(), http.StatusUnprocessableEntity)
		return
	}
	if checked.document == nil {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusUnprocessableEntity)
		w.Write(checked.answer)
		return
	}
	env := store.Envelope{Kind: kind, SchemaVersion: entry.current(), ProducedBy: checked.module}
	from := ""
	if found {
		from = head.Hash
	}
	h, seq, err := s.store.CAS(name, from, []byte(*checked.document), env, checked.documentHash, writer, nowMs)
	if err != nil {
		var ce *store.ConflictError
		if asConflict(err, &ce) {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusConflict)
			json.NewEncoder(w).Encode(map[string]any{"current": ce.Current, "seq": ce.Seq})
			return
		}
		fail(w, err, http.StatusInternalServerError)
		return
	}
	flagUnknown(w, name, writer, checked.unknownFields)
	var warned struct {
		Warnings json.RawMessage `json:"warnings"`
	}
	json.Unmarshal(checked.answer, &warned)
	answer := map[string]any{"name": name, "hash": h, "seq": seq}
	if len(warned.Warnings) > 2 {
		answer["warnings"] = warned.Warnings
	}
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("ETag", `"`+h+`"`)
	json.NewEncoder(w).Encode(answer)
}

func (s *server) handleBlob(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "use GET", http.StatusMethodNotAllowed)
		return
	}
	name := strings.TrimPrefix(r.URL.Path, "/blobs/")
	hash, gz := strings.CutSuffix(name, ".gz")
	etag := `"` + name + `"`
	send := func(plain []byte, compressed func() []byte) {
		w.Header().Set("ETag", etag)
		w.Header().Set("Cache-Control", "max-age=31536000, immutable") // content-addressed: cache forever
		w.Header().Set("Vary", "Accept")
		if r.Header.Get("If-None-Match") == etag {
			w.WriteHeader(http.StatusNotModified)
			return
		}
		if gz || strings.Contains(r.Header.Get("Accept"), "application/gzip") {
			// Content-Type, never Content-Encoding: the page hands the compressed
			// bytes to the modules without unzipping them.
			w.Header().Set("Content-Type", "application/gzip")
			w.Write(compressed())
			return
		}
		w.Header().Set("Content-Type", "application/json")
		w.Write(plain)
	}
	var data []byte
	var env store.Envelope
	err := store.ErrNotFound
	if s.store != nil {
		data, env, err = s.store.GetBlob(hash)
	}
	if err == store.ErrNotFound {
		kind, doc, ok := s.keptDoc(hash)
		if ok {
			version := docVersion(doc)
			w.Header().Set("X-Document-Kind", kind)
			w.Header().Set("X-Schema-Version", strconv.FormatUint(uint64(version), 10))
			if link := s.storedSchemaLink(kind, version); link != "" {
				w.Header().Set("Link", link)
			}
			send(doc, func() []byte { return s.gzippedBlob(hash, doc) })
			return
		}
		if doc, ok := s.publishedBlob(hash); ok {
			if link := s.publishedSchemaLink(doc.tag); link != "" {
				w.Header().Set("Link", link)
			}
			send(doc.body, func() []byte { return doc.gz })
			return
		}
		http.NotFound(w, r)
		return
	}
	if err != nil {
		fail(w, err, http.StatusBadRequest)
		return
	}
	// The policy blob is the authorization structure itself. A client gets
	// the manifest's policy_hash, never the document. The log endpoint is the
	// audited path to policy changes.
	if s.restricted(env.Kind) {
		http.NotFound(w, r)
		return
	}
	w.Header().Set("X-Document-Kind", env.Kind)
	w.Header().Set("X-Schema-Version", fmt.Sprintf("%d", env.SchemaVersion))
	if link := s.storedSchemaLink(env.Kind, env.SchemaVersion); link != "" {
		w.Header().Set("Link", link)
	}
	send(data, func() []byte { return s.gzippedBlob(hash, data) })
}

// gzippedBlob caches the last few compressed blobs: every page that loads
// a published network fetches it, and compressing 10 MB per request would
// use the server's whole budget. Compression is transport only
// (docs/FORMATS.md).
func (s *server) gzippedBlob(hash string, data []byte) []byte {
	s.gzMu.Lock()
	defer s.gzMu.Unlock()
	if gz, ok := s.gzCache[hash]; ok {
		return gz
	}
	gz := gzipped(data)
	if s.gzCache == nil || len(s.gzCache) >= 8 {
		s.gzCache = map[string][]byte{}
	}
	s.gzCache[hash] = gz
	return gz
}

func docVersion(doc []byte) uint32 {
	var tag struct {
		SchemaVersion uint32 `json:"schema_version"`
	}
	if json.Unmarshal(doc, &tag) != nil || tag.SchemaVersion < 1 {
		return 1
	}
	return tag.SchemaVersion
}
