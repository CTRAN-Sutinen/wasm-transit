// Package pointers serves the pointer and blob routes over the
// content-addressed store (docs/architecture/storage.md; routes in
// server/openapi.json). A generic pointer write is checked by its kind's
// module, as the kind table names (server/kinds).
//
// /blobs also answers each log's documents and the published documents
// with or without the store. The edits pointer's log is the pointer's
// history before the disruptions log, read only. The published plan set,
// the picks' networks, and the derived documents are refused here: they
// publish through POST /plans and server/imports.
package pointers

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"

	"rustwasm/server/authz"
	"rustwasm/server/edits"
	"rustwasm/server/httpx"
	"rustwasm/server/imports"
	"rustwasm/server/kinds"
	"rustwasm/server/logs"
	"rustwasm/server/store"
	"rustwasm/server/wasmhost"
)

// Deps is what the host gives the routes.
type Deps struct {
	Store func() *store.Store
	Table func() (*kinds.Table, error)
	Logs  *logs.Logs
	Edits *edits.Edits
	// Fresh is an instance of its own for a write's check, closed after.
	Fresh func(ctx context.Context) (*wasmhost.Instance, error)
	// PublishedBlob is a document the server publishes, by its hash.
	PublishedBlob func(hash string) (imports.Doc, bool)
	// StoredSchemaLink and PublishedSchemaLink are a Link header naming the
	// document's schema, or "".
	StoredSchemaLink    func(kind string, version uint32) string
	PublishedSchemaLink func(tag string) string
}

type Pointers struct {
	store               func() *store.Store
	table               func() (*kinds.Table, error)
	logs                *logs.Logs
	edits               *edits.Edits
	fresh               func(ctx context.Context) (*wasmhost.Instance, error)
	publishedBlob       func(hash string) (imports.Doc, bool)
	storedSchemaLink    func(kind string, version uint32) string
	publishedSchemaLink func(tag string) string

	gzMu    sync.Mutex
	gzCache map[string][]byte
}

func New(d Deps) *Pointers {
	return &Pointers{store: d.Store, table: d.Table, logs: d.Logs, edits: d.Edits, fresh: d.Fresh, publishedBlob: d.PublishedBlob,
		storedSchemaLink: d.StoredSchemaLink, publishedSchemaLink: d.PublishedSchemaLink}
}

func (p *Pointers) HandlePointers(w http.ResponseWriter, r *http.Request) {
	if p.store() == nil {
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
		p.handlePointerLog(w, r, name)
		return
	}
	if len(parts) != 1 {
		http.Error(w, "bad path", http.StatusBadRequest)
		return
	}
	switch r.Method {
	case http.MethodGet:
		p.handlePointerHead(w, name)
	case http.MethodPost:
		p.handlePointerPost(w, r, name)
	default:
		http.Error(w, "use GET or POST", http.StatusMethodNotAllowed)
	}
}

func (p *Pointers) handlePointerHead(w http.ResponseWriter, name string) {
	if name == "edits" {
		_, head, err := p.edits.Now()
		if err != nil {
			httpx.Fail(w, err, http.StatusInternalServerError)
			return
		}
		head.Name = name
		httpx.LabelStale(w, head)
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(head)
		return
	}
	head, found, err := p.store().Head(name)
	if err != nil {
		httpx.Fail(w, err, http.StatusInternalServerError)
		return
	}
	if !found {
		http.Error(w, "no such pointer", http.StatusNotFound)
		return
	}
	httpx.LabelStale(w, head)
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(head)
}

func (p *Pointers) handlePointerLog(w http.ResponseWriter, r *http.Request, name string) {
	since := r.URL.Query().Get("since")
	entries, found, err := p.store().Log(name, since)
	if err != nil {
		httpx.Fail(w, err, http.StatusInternalServerError)
		return
	}
	// Writer identities leave the store only for audit-granted callers.
	canAudit := authz.CanAudit(r)
	if !canAudit {
		for i := range entries {
			entries[i].Writer = ""
		}
	}
	head, _, _ := p.store().Head(name)
	httpx.LabelStale(w, head)
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]any{
		"name":        name,
		"entries":     entries,
		"since_found": found,
		"current":     head.Hash,
		"whole":       !found, // a since outside the log: take the whole current document
	})
}

func (p *Pointers) handlePointerPost(w http.ResponseWriter, r *http.Request, name string) {
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
	writer := httpx.Writer(r, "anonymous")
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
		seq, got, refusal, err := p.edits.Write(r, req.Client, &from)
		if err != nil {
			edits.WriteFailed(w, err)
			return
		}
		if refusal != nil {
			edits.Refused(w, got, refusal)
			return
		}
		httpx.FlagUnknown(w, "edits", writer, got.UnknownFields)
		_, head, err := p.edits.Now()
		if err != nil {
			httpx.Fail(w, err, http.StatusInternalServerError)
			return
		}
		hash := head.Hash
		w.Header().Set("Content-Type", "application/json")
		w.Header().Set("ETag", `"`+hash+`"`)
		json.NewEncoder(w).Encode(map[string]any{"name": name, "hash": hash, "seq": seq})
		return
	}

	head, found, err := p.store().Head(name)
	if err != nil {
		httpx.Fail(w, err, http.StatusInternalServerError)
		return
	}
	kind := req.Kind
	if found {
		kind = head.Envelope.Kind
	} else if kind == "" {
		http.Error(w, "kind is required when creating a pointer", http.StatusBadRequest)
		return
	}

	entry, refusal, err := p.pointerRefusal(name, kind)
	if err != nil {
		httpx.Fail(w, err, http.StatusInternalServerError)
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
	checked, err := p.checkKind(r.Context(), entry, json.RawMessage(req.Document))
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
	env := store.Envelope{Kind: kind, SchemaVersion: entry.Current(), ProducedBy: checked.module}
	from := ""
	if found {
		from = head.Hash
	}
	h, seq, err := p.store().CAS(name, from, []byte(*checked.document), env, checked.documentHash, writer, nowMs)
	if err != nil {
		var ce *store.ConflictError
		if errors.As(err, &ce) {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusConflict)
			json.NewEncoder(w).Encode(map[string]any{"current": ce.Current, "seq": ce.Seq})
			return
		}
		httpx.Fail(w, err, http.StatusInternalServerError)
		return
	}
	httpx.FlagUnknown(w, name, writer, checked.unknownFields)
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

func (p *Pointers) HandleBlob(w http.ResponseWriter, r *http.Request) {
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
	if p.store() != nil {
		data, env, err = p.store().GetBlob(hash)
	}
	if err == store.ErrNotFound {
		kind, doc, ok := p.logs.Doc(hash)
		if ok {
			version := docVersion(doc)
			w.Header().Set("X-Document-Kind", kind)
			w.Header().Set("X-Schema-Version", strconv.FormatUint(uint64(version), 10))
			if link := p.storedSchemaLink(kind, version); link != "" {
				w.Header().Set("Link", link)
			}
			send(doc, func() []byte { return p.gzippedBlob(hash, doc) })
			return
		}
		if doc, ok := p.publishedBlob(hash); ok {
			if link := p.publishedSchemaLink(doc.Tag); link != "" {
				w.Header().Set("Link", link)
			}
			send(doc.Body, func() []byte { return doc.Gz })
			return
		}
		http.NotFound(w, r)
		return
	}
	if err != nil {
		httpx.Fail(w, err, http.StatusBadRequest)
		return
	}
	// The policy blob is the authorization structure itself. A client gets
	// the manifest's policy_hash, never the document. The log endpoint is the
	// audited path to policy changes.
	if p.restricted(env.Kind) {
		http.NotFound(w, r)
		return
	}
	w.Header().Set("X-Document-Kind", env.Kind)
	w.Header().Set("X-Schema-Version", fmt.Sprintf("%d", env.SchemaVersion))
	if link := p.storedSchemaLink(env.Kind, env.SchemaVersion); link != "" {
		w.Header().Set("Link", link)
	}
	send(data, func() []byte { return p.gzippedBlob(hash, data) })
}

// gzippedBlob caches the last few compressed blobs: every page that loads
// a published network fetches it, and compressing 10 MB per request would
// use the server's whole budget. Compression is transport only
// (docs/FORMATS.md).
func (p *Pointers) gzippedBlob(hash string, data []byte) []byte {
	p.gzMu.Lock()
	defer p.gzMu.Unlock()
	if gz, ok := p.gzCache[hash]; ok {
		return gz
	}
	gz := imports.Gzipped(data)
	if p.gzCache == nil || len(p.gzCache) >= 8 {
		p.gzCache = map[string][]byte{}
	}
	p.gzCache[hash] = gz
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

// NameOf is the pointer a /pointers/ request names.
func NameOf(r *http.Request) string {
	rest := strings.TrimPrefix(r.URL.Path, "/pointers/")
	return strings.Split(rest, "/")[0]
}

// With no table, the policy is restricted: a missing table never opens it.
func (p *Pointers) restricted(kind string) bool {
	t, err := p.table()
	if err != nil {
		return kind == "policy"
	}
	e := t.ByStoreKind(kind)
	return e == nil && kind == "policy" || e != nil && e.Read == "restricted"
}

func (p *Pointers) pointerRefusal(name, kind string) (*kinds.Entry, string, error) {
	t, err := p.table()
	if err != nil {
		return nil, "", err
	}
	e, refusal := t.PointerRefusal(name, kind)
	return e, refusal, nil
}

type kindCheck struct {
	answer        json.RawMessage
	document      *string
	documentHash  string
	unknownFields []string
	module        string
}

// A pointer write is rare, so the module instance is fresh and closed
// after.
func (p *Pointers) checkKind(ctx context.Context, e *kinds.Entry, doc json.RawMessage) (kindCheck, error) {
	inst, err := p.fresh(ctx)
	if err != nil {
		return kindCheck{}, fmt.Errorf("an instance for %s: %w", e.Module, err)
	}
	defer inst.Close(ctx)
	host := inst.As(e.Module)
	answer, err := host.InvokeRaw(ctx, map[string]any{"op": e.Validate, e.Tag: doc})
	if err != nil {
		return kindCheck{}, err
	}
	var out struct {
		Document      *string  `json:"document"`
		DocumentHash  string   `json:"document_hash"`
		UnknownFields []string `json:"unknown_fields"`
	}
	if err := json.Unmarshal(answer, &out); err != nil {
		return kindCheck{}, err
	}
	return kindCheck{answer: answer, document: out.Document, documentHash: out.DocumentHash, unknownFields: out.UnknownFields, module: "cap-" + e.Module + "@" + host.Hash()}, nil
}
