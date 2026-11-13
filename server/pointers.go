package main

// The generic store surface (store mode, -store <url>): pointers and
// blobs over the content-addressed store (docs/FORMATS.md,
// docs/architecture/server.md). /edits is the edit_records
// specialization of POST /pointers/edits (edits.go).
//
//	GET  /pointers/{name}              -> {name, hash, seq, envelope} (404 when absent)
//	GET  /pointers/{name}/log?since=H  -> {entries, since_found, current}
//	POST /pointers/{name}              -> the CAS: 200 + new head, 409 + {current}
//	GET  /blobs/{hash}                 -> raw bytes, ETag the hash, envelope in headers;
//	                                      Accept: application/gzip -> the same bytes gzipped
//	                                      (transport only), for a page that registers a
//	                                      document compressed (kernel op 256)
//	GET  /blobs/{hash}.gz              -> the same gzipped bytes by path, the layout a
//	                                      static host serves too (export.go)
//
// /blobs also answers the published documents (the network, its
// boundary, the streets, the walk table — publishedBlob) with or
// without the store.
//
// POST bodies: for kind edit_records, {"from", "client": <document>}
// — the server brings an older document forward (cap-migrate) and runs
// cap-sync's read_edits on both sides and sync_edits (receive-clamped, D-56), and stores the canonical
// converged bytes. The published plan set and the picks' networks
// (kinds plans and network_document, pointers plans and plan.<id>) are
// refused here: they publish through POST /plans (plans.go). Other
// kinds take {"from", "kind"? (on create),
// "document": "<raw text>", "document_hash"?} verbatim; document_hash,
// when present, is asserted against sha256 of the bytes.

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"

	"rustwasm/server/store"
)

// editsVersion is the edit document version the hosted modules read
// (transit_sync::SCHEMA_VERSION); a document in an older one comes
// forward through cap-migrate at receive.
const editsVersion = 2

// networkVersion is the network document version the hosted modules
// read and cap-network writes (transit_network::SCHEMA_VERSION).
const networkVersion = 2

// emptyEdits is the edit document with no records, in that version.
const emptyEdits = `{"kind":"edit_records","schema_version":2,"records":[]}`

// syncEnvelope is the envelope every edit_records commit carries: the
// kind, the schema version, and the producing module's hash — the
// conflict function is content-addressed with it.
func (s *server) syncEnvelope() store.Envelope {
	return store.Envelope{
		Kind:          "edit_records",
		SchemaVersion: editsVersion,
		ProducedBy:    "cap-sync@" + s.wasm.Hash(),
	}
}

// defaultMigrateWasm is where `just caps` puts the migration module.
const defaultMigrateWasm = "../web/pkg/cap_migrate.wasm"

// migrator is the hosted migration capability (cap-migrate), loaded on
// first use: most receives are already current and never need it.
func (s *server) migrator() (*WasmHost, error) {
	s.migrateOnce.Do(func() {
		path := s.migrateWasm
		if path == "" {
			path = defaultMigrateWasm
		}
		s.migrateHost, s.migrateErr = NewWasmHost(context.Background(), path)
	})
	return s.migrateHost, s.migrateErr
}

// forward brings a document of kind to the version the hosted modules
// read, through cap-migrate: a current (or newer) document comes back
// as it is — a newer one is refused by the module that reads it,
// naming both versions, never rewritten here. migrated reports whether
// it moved; the answer is the migrated document's canonical bytes.
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

// syncOp invokes the sync module. The host serializes invokes already
// (WasmHost.call); the module is not reentrant.
func (s *server) syncOp(req map[string]any, out any) error {
	return s.wasm.Invoke(context.Background(), req, out)
}

// readEdits reads a received document at the receive boundary — an
// older version brought forward by cap-migrate first — and returns its
// records plus the canonical document bytes and hash.
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

// mergeEdits runs the sync protocol's merge step over the receive path:
// both documents migrate at receive, the client's clock is clamped to
// the server's (D-56), merge + submit run in the module, and the answer
// is the canonical converged document and its hash.
func (s *server) mergeEdits(clientRaw, serverRaw json.RawMessage, nowMs int64) (document string, hash string, err error) {
	clientRec, _, _, _, err := s.readEdits(clientRaw)
	if err != nil {
		return "", "", fmt.Errorf("read_edits client: %w", err)
	}
	serverRec, _, _, _, err := s.readEdits(serverRaw)
	if err != nil {
		return "", "", fmt.Errorf("read_edits server: %w", err)
	}
	var resp struct {
		Document     string `json:"document"`
		DocumentHash string `json:"document_hash"`
	}
	if err := s.syncOp(map[string]any{
		"op": "sync_edits", "client": clientRec, "server": serverRec, "now_ms": nowMs,
	}, &resp); err != nil {
		return "", "", fmt.Errorf("sync_edits: %w", err)
	}
	return resp.Document, resp.DocumentHash, nil
}

// commitEdits CAS-commits a merged edit document onto the edits
// pointer, retrying against a moved pointer. Returns the response body
// (the canonical document) and its hash.
func (s *server) commitEdits(clientRaw json.RawMessage, writer string) (string, string, error) {
	nowMs := time.Now().UnixMilli()
	for attempt := 0; attempt < 5; attempt++ {
		head, found, err := s.store.Head("edits")
		if err != nil {
			return "", "", err
		}
		serverRaw := json.RawMessage(emptyEdits)
		from := ""
		if found {
			from = head.Hash
			body, _, err := s.store.GetBlob(head.Hash)
			if err != nil {
				return "", "", err
			}
			serverRaw = body
		}
		document, hash, err := s.mergeEdits(clientRaw, serverRaw, nowMs)
		if err != nil {
			return "", "", err
		}
		_, _, err = s.store.CAS("edits", from, []byte(document), s.syncEnvelope(), hash, writer, nowMs)
		if err != nil {
			var ce *store.ConflictError
			if asConflict(err, &ce) && attempt < 4 {
				continue // re-read, re-merge, retry
			}
			return "", "", err
		}
		return document, hash, nil
	}
	return "", "", fmt.Errorf("edits pointer moved on every attempt")
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
	// Writer identities leave the store only for audit-granted callers
	// (auth on); the log stays redacted for everyone else.
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

	// The published set and the picks' networks are written only by
	// POST /plans (plans.go): their identity is a module's, and a raw
	// write would skip cap-network's validation and cap-plans' chain.
	if kind == "plans" || kind == "network_document" || name == "plans" || strings.HasPrefix(name, "plan.") {
		http.Error(w, "picks publish through POST /plans", http.StatusBadRequest)
		return
	}
	// The stops document is written only by POST /stops (stops.go): a
	// raw write would skip cap-stops' validation and the picks' refresh.
	if kind == "stops" || name == stopsPointer {
		http.Error(w, "the stops document is written through POST /stops", http.StatusBadRequest)
		return
	}
	// A build names files the store must hold: builds land from -dist,
	// and the pointer moves only to a build its log holds (POST /builds).
	if kind == store.BuildKind || kind == store.BuildFileKind || name == buildPointer {
		http.Error(w, "builds land from -dist and move through POST /builds", http.StatusBadRequest)
		return
	}

	if kind == "edit_records" {
		if len(req.Client) == 0 {
			http.Error(w, "edit_records takes a client document", http.StatusBadRequest)
			return
		}
		// One CAS attempt on the client's last-seen hash; the caller
		// retries on 409 (POST /edits retries internally).
		serverRaw := json.RawMessage(emptyEdits)
		from := req.From
		if found {
			b, _, err := s.store.GetBlob(head.Hash)
			if err != nil {
				fail(w, err, http.StatusInternalServerError)
				return
			}
			serverRaw = b
		}
		document, hash, err := s.mergeEdits(req.Client, serverRaw, nowMs)
		if err != nil {
			fail(w, err, http.StatusBadRequest)
			return
		}
		h, seq, err := s.store.CAS(name, from, []byte(document), s.syncEnvelope(), hash, writer, nowMs)
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
		w.Header().Set("Content-Type", "application/json")
		w.Header().Set("ETag", `"`+h+`"`)
		json.NewEncoder(w).Encode(map[string]any{"name": name, "hash": h, "seq": seq})
		return
	}

	// Host-owned document kinds (manifest, policy): raw bytes verbatim,
	// hashed by the host with the shared formula (no module claims
	// their identity), document_hash asserted when present.
	if req.Document == "" {
		http.Error(w, "document is required", http.StatusBadRequest)
		return
	}
	data := []byte(req.Document)
	producedBy := r.Header.Get("X-Produced-By")
	if producedBy == "" {
		producedBy = "server"
	}
	env := store.Envelope{Kind: kind, SchemaVersion: 1, ProducedBy: producedBy}
	from := ""
	if found {
		from = head.Hash
	}
	h, seq, err := s.store.CAS(name, from, data, env, req.DocumentHash, writer, nowMs)
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
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("ETag", `"`+h+`"`)
	json.NewEncoder(w).Encode(map[string]any{"name": name, "hash": h, "seq": seq})
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
			// Content-Type, never Content-Encoding: the page hands the
			// compressed bytes to the modules without unzipping them.
			w.Header().Set("Content-Type", "application/gzip")
			w.Write(compressed())
			return
		}
		w.Header().Set("Content-Type", "application/json")
		w.Write(plain)
	}
	// The store's blob first (it carries its envelope), else the
	// published document of that hash.
	var data []byte
	var env store.Envelope
	err := store.ErrNotFound
	if s.store != nil {
		data, env, err = s.store.GetBlob(hash)
	}
	if err == store.ErrNotFound {
		if doc, ok := s.publishedBlob(hash); ok {
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
	// Server-internal config documents are never served to clients:
	// the policy blob is the authorization structure itself (roles,
	// rules, pointers) — a client gets the manifest's policy_hash
	// (opaque, for rotation detection), never the document. The log
	// endpoint is the audited path to policy CHANGES, gated by auth.
	if env.Kind == "policy" {
		http.NotFound(w, r)
		return
	}
	w.Header().Set("X-Document-Kind", env.Kind)
	w.Header().Set("X-Schema-Version", fmt.Sprintf("%d", env.SchemaVersion))
	send(data, func() []byte { return s.gzippedBlob(hash, data) })
}

// gzippedBlob compresses a blob for transport, caching the last few:
// a published network is fetched by every page that loads it, and
// compressing 10 MB per request would be the server's whole budget.
// Never durable — compression is transport only (docs/FORMATS.md).
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
