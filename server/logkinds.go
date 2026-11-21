package main

// The logs that a capability writes whole (D-228; docs/architecture/
// server.md, "One path for a log"): each log whose kinds the kind table
// names with a receiving capability (`receive`). The host reads the
// head, makes one call, and creates the entry. It decides nothing:
//
//   - the fold's documents are each kind's capability's `log_document`
//     over the fold's records of its kind;
//   - a write is the receiving capability's `receive` op over the
//     documents at the head, which runs the write's ops, checks them,
//     and answers the entry and the documents it wrote, or why it adds
//     nothing;
//   - the host checks the write rule of each document the op answers it
//     wrote (the rule is named by the kind's store kind), stores the
//     entry with a conditional create, and asks again over the new head
//     when another write took the seq.
//
//	POST /logs/<log>  body: {"key", "ops": [{"cap", "op", "args"}], "read_seq"?, "claimed_at_ms"?}
//	                  200 {seq, <store kind>_hash for each kind, landed?}
//	POST <a kind's own route> (the kind table's `post`): one op of the
//	                  kind's checking capability over the body
//	                  200 {<answer>: the document after the write, <echo>...}
//	                  422 {op, refusals}   409 {current, seq} | {op, conflict} | {conflict, overlap, seq}
//
// A page writes by intent (D-189): it sends what the person asked for,
// never the next document. A key that landed answers the entry that
// holds it, so a resend lands once. `read_seq` is the log's seq whose
// documents the page read: a record the write changes that a later
// entry changed is a 409 for a person to settle, and a change to
// another record does not refuse it. `claimed_at_ms` is the device's
// clock when the person made the write, kept in the entry's intent
// beside the server's stamp (D-222).
//
// The manifest pins each kind's document as `<store kind>_hash`, and
// /blobs serves it by that hash. An empty log takes its first entry from
// its kinds' old pointers (the kind table's `pointers`; seedKept).
// Writes the server makes itself (the feed's differences, the work
// order system's answer) take the same path (receiveLog).

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"sort"
	"strings"
	"sync"
	"time"

	"rustwasm/server/store"
)

// logDoc is one document a log's fold holds: its hash and its canonical
// bytes ("" when the fold holds none of its records).
type logDoc struct {
	hash string
	doc  []byte
}

// keptLog is a log a receive op writes: its fold, the kinds it holds (in
// the kind table's order), the capability whose receive op writes it,
// and each kind's document at the head, by store kind (fold.mu held).
type keptLog struct {
	name    string
	receive string
	kinds   []kindEntry
	fold    *logFold
	docs    map[string]logDoc
	// intent is whether a page writes the log by intent.
	intent bool
}

// heldModule is the server's one instance of a capability, made on first
// use, and the kernel kinds it declares (its `about`).
type heldModule struct {
	mu        sync.Mutex
	host      *WasmHost
	documents map[string]bool
}

// opAbout is the kernel's about: the capabilities a module links, each
// with the documents it declares.
const opAbout = 268

// modulePath is where a capability's module is: the file its own flag
// names (-<cap>-wasm), else cap_<id>.wasm in -modules (by default where
// `just caps` writes it).
func (s *server) modulePath(capability string) string {
	if p := s.checkWasm[capability]; p != "" {
		return p
	}
	if p := s.moduleFiles[capability]; p != "" {
		return p
	}
	if p := s.serverModules[capability]; p != "" {
		return p
	}
	dir := s.modulesDir
	if dir == "" {
		dir = "../web/pkg"
	}
	return moduleIn(dir, capability)
}

// serverCapabilities are the capabilities the server runs for itself,
// whatever kinds it keeps: the edit merge, the feed and street imports,
// the plan chain, migration, and the logs.
var serverCapabilities = []string{"sync", "network", "streets", "plans", "migrate", "log"}

// hostedModules are the modules this server hosts, by capability, as
// each build keeps them: the server's own, every capability the kind
// table names (a kind's module and its receive op's), and cap-authz
// when sign-in is on. A new kind's capability joins from its line.
func (s *server) hostedModules(authz bool) map[string]string {
	ids := append([]string{}, serverCapabilities...)
	if t, err := s.kindTable(); err == nil {
		for _, e := range t.Kinds {
			ids = append(ids, e.Module, e.Receive)
		}
	}
	if authz {
		ids = append(ids, "authz")
	}
	out := map[string]string{}
	for _, id := range ids {
		if id != "" {
			out[id] = s.modulePath(id)
		}
	}
	return out
}

// withModule runs `use` on the server's one instance of a capability,
// then empties it (the kernel's evict), so no document outlives the
// request that sent it.
func (s *server) withModule(ctx context.Context, capability string, use func(*WasmHost) error) error {
	return s.withModuleOf(ctx, capability, func(host *WasmHost, _ map[string]bool) error { return use(host) })
}

// withModuleOf is withModule, with the kernel kinds the module declares.
func (s *server) withModuleOf(ctx context.Context, capability string, use func(*WasmHost, map[string]bool) error) error {
	s.modulesMu.Lock()
	if s.modules == nil {
		s.modules = map[string]*heldModule{}
	}
	m := s.modules[capability]
	if m == nil {
		m = &heldModule{}
		s.modules[capability] = m
	}
	s.modulesMu.Unlock()
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.host == nil {
		host, err := NewWasmHost(ctx, s.modulePath(capability))
		if err != nil {
			return fmt.Errorf("the %s module: %w", capability, err)
		}
		var about struct {
			Capabilities []struct {
				Documents []string `json:"documents"`
			} `json:"capabilities"`
		}
		if err := host.InvokeBytes(ctx, opAbout, nil, &about); err != nil {
			host.Close(ctx)
			return fmt.Errorf("the %s module's about: %w", capability, err)
		}
		m.documents = map[string]bool{}
		for _, c := range about.Capabilities {
			for _, d := range c.Documents {
				m.documents[d] = true
			}
		}
		m.host = host
	}
	err := use(m.host, m.documents)
	var evicted json.RawMessage
	if evictErr := m.host.InvokeBytes(ctx, opEvict, []byte(`{"keep":[]}`), &evicted); evictErr != nil && err == nil {
		err = fmt.Errorf("emptying the %s module: %w", capability, evictErr)
	}
	return err
}

// keptLogs are the logs a receive op writes, by name, from the kind
// table. None without a store or a table.
func (s *server) keptLogs() map[string]*keptLog {
	s.keptOnce.Do(func() {
		s.kept = map[string]*keptLog{}
		t, err := s.kindTable()
		if err != nil || s.store == nil {
			return
		}
		for _, e := range t.Kinds {
			if e.Log == "" || e.Receive == "" {
				continue
			}
			l := s.kept[e.Log]
			if l == nil {
				l = &keptLog{name: e.Log, receive: e.Receive, docs: map[string]logDoc{}}
				l.intent = e.Intent == nil || *e.Intent
				l.fold = &logFold{name: e.Log, assemble: func(records map[string]json.RawMessage) error { return s.assembleKept(l, records) }, documents: func() map[string]string {
					hashes := map[string]string{}
					for _, k := range l.kinds {
						hashes[k.StoreKind+"_hash"] = l.docs[k.StoreKind].hash
					}
					// A log of one kind also names it `document`.
					if len(l.kinds) == 1 {
						hashes["document"] = l.docs[l.kinds[0].StoreKind].hash
					}
					return hashes
				}}
				s.kept[e.Log] = l
			}
			l.kinds = append(l.kinds, e)
			// A kind whose merge is LWW writes back its own records on
			// a revert, stamped to win the merge (D-193): its module's
			// log_revert.
			if e.Merge == "lww" && e.Module != "" {
				module := e.Module
				l.fold.revise = func(states json.RawMessage, nowMs int64) (json.RawMessage, error) {
					var out struct {
						Written json.RawMessage `json:"written"`
					}
					ctx := context.Background()
					err := s.withModule(ctx, module, func(host *WasmHost) error {
						return host.Invoke(ctx, map[string]any{"op": "log_revert", "states": states, "now_ms": nowMs}, &out)
					})
					return out.Written, err
				}
			}
		}
	})
	return s.kept
}

// errNoLog is the error of a log the kind table does not name.
func errNoLog(name string) error {
	return fmt.Errorf("the kind table names no %s log (run `just caps`)", name)
}

// keptRoutesOf names the routes that write the log's kinds, from the
// kind table.
func (s *server) keptRoutesOf(name string) string {
	var routes []string
	seen := map[string]bool{}
	if l := s.keptLog(name); l != nil {
		for _, k := range l.kinds {
			if k.Route != "" && !seen[k.Route] {
				seen[k.Route] = true
				routes = append(routes, k.Route)
			}
		}
	}
	return strings.Join(routes, "; ")
}

// keptLog is the log `name` a receive op writes, or nil.
func (s *server) keptLog(name string) *keptLog {
	return s.keptLogs()[name]
}

// keptNames are the names of the logs a receive op writes, in order.
func (s *server) keptNames() []string {
	var names []string
	for name := range s.keptLogs() {
		names = append(names, name)
	}
	sort.Strings(names)
	return names
}

// assembleKept turns the fold's records into each kind's document,
// through the kind's capability (`log_document`). Caller holds the
// fold's lock.
func (s *server) assembleKept(l *keptLog, records map[string]json.RawMessage) error {
	ctx := context.Background()
	l.docs = map[string]logDoc{}
	for _, k := range l.kinds {
		if len(k.Records) == 0 || k.Module == "" {
			continue
		}
		// A document is there when the fold holds a record of its
		// collection, or its own record. A kind whose merge is LWW
		// always has one: the empty log's is the empty document.
		raw, ok := records[k.Records[0].Kind]
		own := ownRecord(records, k.StoreKind)
		if (!ok || len(raw) <= 2) && own == nil && k.Merge != "lww" {
			continue
		}
		if !ok {
			raw = json.RawMessage("{}")
		}
		req := map[string]any{"op": "log_document", "kind": k.StoreKind, "records": raw}
		if own != nil {
			req["document"] = own
		}
		var out struct {
			DocumentHash string `json:"document_hash"`
			Document     string `json:"document"`
		}
		if err := s.withModule(ctx, k.Module, func(host *WasmHost) error { return host.Invoke(ctx, req, &out) }); err != nil {
			return fmt.Errorf("the %s document: %w", k.StoreKind, err)
		}
		l.docs[k.StoreKind] = logDoc{out.DocumentHash, []byte(out.Document)}
	}
	return nil
}

// keptHead is a kind's document in force: its hash and bytes, or found
// false when there is none (or no store, or the kind is not in such a
// log).
func (s *server) keptHead(storeKind string) (hash string, doc []byte, found bool, err error) {
	for _, l := range s.keptLogs() {
		for _, k := range l.kinds {
			if k.StoreKind != storeKind {
				continue
			}
			if _, err := s.refreshLog(l.fold); err != nil {
				return "", nil, false, err
			}
			l.fold.mu.Lock()
			defer l.fold.mu.Unlock()
			held := l.docs[storeKind]
			return held.hash, held.doc, held.hash != "", nil
		}
	}
	return "", nil, false, nil
}

// keptDoc answers the document of this hash, if a fold holds it: what
// /blobs serves for the manifest's pins.
func (s *server) keptDoc(hash string) (kind string, doc []byte, ok bool) {
	if hash == "" {
		return "", nil, false
	}
	for _, l := range s.keptLogs() {
		l.fold.mu.Lock()
		for k, held := range l.docs {
			if held.hash == hash {
				l.fold.mu.Unlock()
				return k, held.doc, true
			}
		}
		l.fold.mu.Unlock()
	}
	return "", nil, false
}

// keptPins reads each log a receive op writes: each kind's document
// (`<store kind>_hash`), and each log's seq, under the log's one lock.
func (s *server) keptPins() (pins map[string]string, seqs map[string]int64) {
	pins, seqs = map[string]string{}, map[string]int64{}
	for _, name := range s.keptNames() {
		l := s.keptLog(name)
		if _, err := s.refreshLog(l.fold); err != nil {
			continue
		}
		l.fold.mu.Lock()
		for _, k := range l.kinds {
			if held := l.docs[k.StoreKind]; held.hash != "" {
				pins[k.StoreKind+"_hash"] = held.hash
			}
		}
		if l.fold.seq > 0 {
			seqs[l.name] = l.fold.seq
		}
		l.fold.mu.Unlock()
	}
	return pins, seqs
}

// ownRecord is a document's own record in the fold's records of its
// kind (`document`), or nil.
func ownRecord(records map[string]json.RawMessage, kind string) json.RawMessage {
	var byID map[string]json.RawMessage
	if raw, ok := records[kind]; ok {
		json.Unmarshal(raw, &byID)
	}
	return byID["document"]
}

// keptLogOf is the log a receive op writes that holds the store kind, or
// nil.
func (s *server) keptLogOf(storeKind string) *keptLog {
	for _, l := range s.keptLogs() {
		for _, k := range l.kinds {
			if k.StoreKind == storeKind {
				return l
			}
		}
	}
	return nil
}

// ruleOf is the write rule of a store kind the log holds (the kind's
// rule; the store kind itself when the log holds no such kind).
func (l *keptLog) ruleOf(storeKind string) string {
	for _, k := range l.kinds {
		if k.StoreKind == storeKind {
			return k.rule()
		}
	}
	return storeKind
}

// loadCode is the kernel's load of a store kind the log holds (0 when
// the table names none).
func (l *keptLog) loadCode(storeKind string) uint32 {
	for _, k := range l.kinds {
		if k.StoreKind == storeKind && k.Load != nil {
			return k.Load.Code
		}
	}
	return 0
}

// keptSeq is the seq of the log that holds the store kind's document in
// force (0 when there is none): the entry a reader of the document read.
func (s *server) keptSeq(storeKind string) int64 {
	l := s.keptLogOf(storeKind)
	if l == nil {
		return 0
	}
	if _, err := s.refreshLog(l.fold); err != nil {
		return 0
	}
	l.fold.mu.Lock()
	defer l.fold.mu.Unlock()
	if l.docs[storeKind].hash == "" {
		return 0
	}
	return l.fold.seq
}

// received is a receive op's answer.
type received struct {
	Entry   string      `json:"entry"`
	Changes int         `json:"changes"`
	Landed  *int64      `json:"landed"`
	Overlap *logOverlap `json:"overlap"`
	// Conflict is the record an op names, changed since the writer read
	// it.
	Conflict *struct {
		Op int    `json:"op"`
		ID string `json:"id"`
	} `json:"conflict"`
	Refused *struct {
		Op       int             `json:"op"`
		Refusals json.RawMessage `json:"refusals"`
	} `json:"refused"`
	Moved *struct {
		Current string `json:"current"`
	} `json:"moved"`
	Failed *struct {
		Op    int    `json:"op"`
		Error string `json:"error"`
	} `json:"failed"`
	UnknownFields []string          `json:"unknown_fields"`
	Heads         map[string]string `json:"heads"`
	// Made are the documents the write made, stored before its entry
	// (a pick's network, as the head of plan.<id>).
	Made []madeDocument `json:"made"`
	// Answer is what the op answers beside the entry, for the route.
	Answer json.RawMessage `json:"answer"`
}

// madeDocument is a document a write made: its canonical bytes under
// their hash, its kind and version for the envelope, and the pointer it
// becomes the head of ("": a blob only).
type madeDocument struct {
	Kind         string `json:"kind"`
	Version      uint32 `json:"version"`
	Document     string `json:"document"`
	DocumentHash string `json:"document_hash"`
	Pointer      string `json:"pointer"`
}

// loadError is a document the host registered before the write that
// the module refused: the caller's input, not the server's failure.
type loadError struct{ err error }

func (e *loadError) Error() string { return e.err.Error() }
func (e *loadError) Unwrap() error { return e.err }

// receiveCall is a write's call beside its entry's header: its ops, the
// documents the host registers in the receiving module first (a feed's
// network), and the documents it runs over in place of the fold's (an
// empty log's first entry, from the old pointers), by store kind. A
// kind with a kernel load is registered, and the op gets its hash in
// `heads`; a kind with none is given inline, in `documents`. Each load's
// hash goes to the op in `loaded`, by the load's answer field
// (`network_hash`).
type receiveCall struct {
	ops   []json.RawMessage
	loads []hostLoad
	heads map[string][]byte
	// onLoad is given each load's answer (a network's unknown fields,
	// its zone).
	onLoad func(answer map[string]any)
	// opsAfterLoads, when set, gives the ops after the loads answered,
	// in place of ops: an op's argument read from a load's answer (a
	// pick's "today" in the network's zone).
	opsAfterLoads func() []json.RawMessage
}

// hostLoad is a document the host registers on a kernel load.
type hostLoad struct {
	code uint32
	doc  []byte
}

// loadedHash is the hash a kernel load answers: its one `<name>_hash`
// field.
func loadedHash(answer map[string]any) string {
	for k, v := range answer {
		if h, ok := v.(string); ok && strings.HasSuffix(k, "_hash") {
			return h
		}
	}
	return ""
}

// receiveLog runs one write to a log through its receive op into one
// entry: the documents at the head registered in the receiving module,
// one call, the write rule of each document it wrote, and the entry
// created at the next seq, asked again over the new head when another
// write took it (D-190, rule 2). The answer is the entry's seq (the
// head's when nothing changed), and landed when the write's key landed
// before. `r` is the person's request, or nil when the server makes the
// write itself or the route checked its write rule already: then no
// write rule is asked, and no name is given.
func (s *server) receiveLog(ctx context.Context, r *http.Request, l *keptLog, w logWrite, call receiveCall) (seq int64, landed bool, got received, refusal *logRefusal, err error) {
	f := l.fold
	f.mu.Lock()
	defer f.mu.Unlock()
	for attempt := 0; attempt < 5; attempt++ {
		if err := s.refreshLogLocked(f); err != nil {
			return 0, false, got, nil, err
		}
		if f.stale {
			return 0, false, got, nil, errStaleHead(f)
		}
		if len(f.gaps) > 0 {
			return 0, false, got, nil, &store.GapError{Log: f.name, Gaps: f.gaps}
		}
		intent := map[string]any{"op": w.op}
		for k, v := range map[string]string{"args_hash": w.argsHash, "key": w.key, "note": w.note} {
			if v != "" {
				intent[k] = v
			}
		}
		if w.claimedAtMs > 0 {
			intent["claimed_at_ms"] = w.claimedAtMs
		}
		write := map[string]any{"header": map[string]any{"log": f.name, "seq": f.seq + 1, "writer": w.writer, "at_ms": time.Now().UnixMilli(), "intent": intent}}
		if len(f.base) > 0 {
			write["checkpoint"] = f.base
		}
		// The entries the writer did not read: a record the write
		// changes that one of them changed is for a person to settle.
		if w.readSeq != nil && *w.readSeq < f.seq {
			unread, err := s.store.Entries(f.name, max(*w.readSeq, 0), f.seq)
			if err != nil {
				return 0, false, got, nil, err
			}
			write["unread"] = unread
		}
		got = received{}
		err := s.withModuleOf(ctx, l.receive, func(host *WasmHost, declares map[string]bool) error {
			write["header"].(map[string]any)["module"] = "cap-" + l.receive + "@" + host.Hash()
			loadedHashes := map[string]string{}
			for _, load := range call.loads {
				var loaded map[string]any
				if err := host.InvokeBytes(ctx, load.code, load.doc, &loaded); err != nil {
					return &loadError{err}
				}
				for k, v := range loaded {
					if h, ok := v.(string); ok && strings.HasSuffix(k, "_hash") {
						loadedHashes[k] = h
					}
				}
				if call.onLoad != nil {
					call.onLoad(loaded)
				}
			}
			heads := map[string]string{}
			documents := map[string]json.RawMessage{}
			for _, k := range l.kinds {
				doc := l.docs[k.StoreKind].doc
				if call.heads != nil {
					doc = call.heads[k.StoreKind]
				}
				if len(doc) == 0 {
					continue
				}
				// A kind the module declares is registered on its kernel
				// load; any other goes inline.
				if k.Load == nil || !declares[strings.TrimPrefix(k.Load.Op, "load_")] {
					documents[k.StoreKind] = json.RawMessage(doc)
					continue
				}
				var loaded map[string]any
				if err := host.InvokeBytes(ctx, k.Load.Code, doc, &loaded); err != nil {
					return fmt.Errorf("the %s document at the head: %w", k.StoreKind, err)
				}
				heads[k.StoreKind] = loadedHash(loaded)
			}
			ops := call.ops
			if call.opsAfterLoads != nil {
				ops = call.opsAfterLoads()
			}
			req := map[string]any{"op": "receive", "write": write, "heads": heads, "ops": ops}
			if len(documents) > 0 {
				req["documents"] = documents
			}
			if len(loadedHashes) > 0 {
				req["loaded"] = loadedHashes
			}
			if r != nil {
				req["by"] = nameOf(r, "anonymous")
			}
			return host.Invoke(ctx, req, &got)
		})
		if err != nil {
			return 0, false, got, nil, err
		}
		switch {
		case got.Landed != nil:
			return *got.Landed, true, got, nil, nil
		case got.Failed != nil:
			return 0, false, got, &logRefusal{http.StatusBadRequest, map[string]any{"op": got.Failed.Op, "error": got.Failed.Error}}, nil
		case got.Refused != nil:
			return 0, false, got, &logRefusal{http.StatusUnprocessableEntity, map[string]any{"op": got.Refused.Op, "refusals": got.Refused.Refusals}}, nil
		case got.Moved != nil:
			return 0, false, got, &logRefusal{http.StatusConflict, map[string]any{"current": got.Moved.Current, "seq": f.seq}}, nil
		case got.Conflict != nil:
			return 0, false, got, &logRefusal{http.StatusConflict, map[string]any{"op": got.Conflict.Op, "conflict": got.Conflict.ID}}, nil
		case got.Overlap != nil:
			o := got.Overlap
			return 0, false, got, &logRefusal{http.StatusConflict, map[string]any{"conflict": o.Kind + " " + o.ID, "overlap": o, "seq": f.seq}}, nil
		}
		// The write rule of each document the write's ops wrote.
		for kind := range got.Heads {
			if r == nil {
				break
			}
			if allow, reason := s.mayWrite(r, l.ruleOf(kind)); !allow {
				return 0, false, got, &logRefusal{http.StatusForbidden, "forbidden: " + reason}, nil
			}
		}
		if got.Entry == "" {
			return f.seq, false, got, nil, nil
		}
		// The documents the write made, stored first: the entry never
		// names a document the store does not hold.
		module := write["header"].(map[string]any)["module"].(string)
		for _, d := range got.Made {
			env := store.Envelope{Kind: d.Kind, SchemaVersion: d.Version, ProducedBy: module}
			if d.Pointer == "" {
				if _, err := s.store.PutBlob([]byte(d.Document), env, d.DocumentHash); err != nil {
					return 0, false, got, nil, err
				}
				continue
			}
			if !store.ValidPointerName(d.Pointer) {
				return 0, false, got, &logRefusal{http.StatusBadRequest, map[string]any{"error": fmt.Sprintf("%q cannot name a pointer (letters, digits, '.', '_', '-')", d.Pointer)}}, nil
			}
			if err := commitHead(s.store, d.Pointer, []byte(d.Document), env, d.DocumentHash, w.writer); err != nil {
				return 0, false, got, nil, err
			}
		}
		err = s.store.AppendEntry(f.name, f.seq+1, []byte(got.Entry))
		var ce *store.EntryConflictError
		if errors.As(err, &ce) {
			continue
		}
		if err != nil {
			return 0, false, got, nil, err
		}
		if err := s.refreshLogLocked(f); err != nil {
			return 0, false, got, nil, err
		}
		return f.seq, false, got, nil, nil
	}
	return 0, false, got, nil, fmt.Errorf("the %s log moved on every attempt", f.name)
}

// writeRefused answers a write that lands nothing: a forbidden one as
// text, any other as JSON.
func writeRefused(w http.ResponseWriter, refusal *logRefusal) {
	if text, ok := refusal.body.(string); ok {
		http.Error(w, text, refusal.status)
		return
	}
	writeJSON(w, refusal.status, refusal.body)
}

// handleKeptIntent is POST /logs/<log> for a log a receive op writes:
// the write's ops, as the page sent them, in one call.
func (s *server) handleKeptIntent(w http.ResponseWriter, r *http.Request, l *keptLog) {
	body, err := io.ReadAll(io.LimitReader(r.Body, 32<<20))
	var req struct {
		Key         string            `json:"key"`
		Ops         []json.RawMessage `json:"ops"`
		ReadSeq     *int64            `json:"read_seq"`
		ClaimedAtMs int64             `json:"claimed_at_ms"`
	}
	if err == nil {
		err = json.Unmarshal(body, &req)
	}
	if err != nil || len(req.Ops) == 0 {
		http.Error(w, `body: {"key": "<idempotency key>", "ops": [{"cap": "...", "op": "...", "args": {...}}], "read_seq": N, "claimed_at_ms": T}`, http.StatusBadRequest)
		return
	}
	write := logWrite{writer: writerOf(r, "anonymous"), op: "POST /logs/" + l.name, argsHash: store.HashOf(body), key: req.Key, readSeq: req.ReadSeq, claimedAtMs: req.ClaimedAtMs}
	seq, landed, got, refusal, err := s.receiveLog(r.Context(), r, l, write, receiveCall{ops: req.Ops})
	if err != nil {
		logWriteFailed(w, err, http.StatusBadRequest)
		return
	}
	if refusal != nil {
		writeRefused(w, refusal)
		return
	}
	flagUnknown(w, "the "+l.name+" log", write.writer, got.UnknownFields)
	answer := map[string]any{"seq": seq}
	l.fold.mu.Lock()
	for _, k := range l.kinds {
		answer[k.StoreKind+"_hash"] = l.docs[k.StoreKind].hash
	}
	l.fold.mu.Unlock()
	if landed {
		answer["landed"] = true
	} else if refs := s.writeReferences(l.fold.name, seq, got.Entry); len(refs) > 0 {
		answer["references"] = refs
	}
	writeJSON(w, http.StatusOK, answer)
}

// keptRoutes are the kinds' own write routes, by path, each with its
// kind and its log.
func (s *server) keptRoutes() map[string]struct {
	kind kindEntry
	log  *keptLog
} {
	out := map[string]struct {
		kind kindEntry
		log  *keptLog
	}{}
	for _, l := range s.keptLogs() {
		for _, k := range l.kinds {
			if k.Post != nil {
				out[k.Post.Path] = struct {
					kind kindEntry
					log  *keptLog
				}{k, l}
			}
		}
	}
	return out
}

// handleKeptRoute serves a kind's own write route: the body as the
// arguments of the route's op (an op of the kind's checking capability),
// run by its log's receive op.
func (s *server) handleKeptRoute(w http.ResponseWriter, r *http.Request) {
	if s.store == nil {
		http.Error(w, "this write needs the store (start with -store <url>)", http.StatusNotFound)
		return
	}
	route, ok := s.keptRoutes()[r.URL.Path]
	if !ok {
		http.NotFound(w, r)
		return
	}
	k, post := route.kind, route.kind.Post
	if r.Method != http.MethodPost {
		http.Error(w, fmt.Sprintf("use POST (clients read the document through the manifest's %s_hash)", k.StoreKind), http.StatusMethodNotAllowed)
		return
	}
	body, err := io.ReadAll(io.LimitReader(r.Body, 64<<20))
	var args map[string]json.RawMessage
	if err == nil {
		err = json.Unmarshal(body, &args)
	}
	if err != nil {
		http.Error(w, "body: a JSON object, the arguments of "+k.Module+" "+post.Op, http.StatusBadRequest)
		return
	}
	op, _ := json.Marshal(map[string]any{"cap": k.Module, "op": post.Op, "args": args})
	write := logWrite{writer: writerOf(r, "anonymous"), op: "POST " + post.Path, argsHash: store.HashOf(body)}
	seq, landed, got, refusal, err := s.receiveLog(r.Context(), r, route.log, write, receiveCall{ops: []json.RawMessage{op}})
	if err != nil {
		logWriteFailed(w, err, http.StatusBadRequest)
		return
	}
	if refusal != nil {
		// The route's one op: its refusals are the route's, and an op
		// that did not run is the request's 400.
		if got.Failed != nil {
			http.Error(w, "body: "+got.Failed.Error, http.StatusBadRequest)
			return
		}
		if body, ok := refusal.body.(map[string]any); ok {
			delete(body, "op")
		}
		writeRefused(w, refusal)
		return
	}
	flagUnknown(w, k.StoreKind, write.writer, got.UnknownFields)
	route.log.fold.mu.Lock()
	hash := route.log.docs[k.StoreKind].hash
	route.log.fold.mu.Unlock()
	if hash == "" {
		hash = got.Heads[k.StoreKind]
	}
	answer := map[string]any{post.Answer: hash}
	for _, field := range post.Echo {
		if v, ok := args[field]; ok {
			answer[field] = v
		}
	}
	if !landed {
		if refs := s.writeReferences(route.log.fold.name, seq, got.Entry); len(refs) > 0 {
			answer["references"] = refs
		}
	}
	writeJSON(w, http.StatusOK, answer)
}

// keptRouteRule is the write rule of a kind's own route: the kind's rule.
func (s *server) keptRouteRule(r *http.Request) string {
	if route, ok := s.keptRoutes()[r.URL.Path]; ok {
		return route.kind.rule()
	}
	return strings.TrimPrefix(r.URL.Path, "/")
}

// seedFile is a file a kind's first entry may come from: its flag, and
// its path ("" for none).
type seedFile struct {
	flag, path string
}

// seedKept gives each empty log a receive op writes its first entry at
// startup: the heads of its kinds' old pointers (the kind table's
// `pointers`), recorded by the receive op's `migrate`. A kind with no
// pointer head takes its file in `files`, when there is one (the dev
// store's -edits file). The writer is migrate:<the first pointer read>,
// or migrate:-<the file's flag>, and the note names each source.
// Nothing when no source exists. The pointers are never written again.
func (s *server) seedKept(files map[string]seedFile) {
	for _, name := range s.keptNames() {
		l := s.keptLog(name)
		head, err := s.store.LogHeadOf(name)
		if err != nil || head.Seq > 0 || len(head.Gaps) > 0 {
			if err != nil {
				log.Printf("seed the %s log: %v", name, err)
			}
			continue
		}
		heads := map[string][]byte{}
		var note, hashes []string
		writer := ""
		for _, k := range l.kinds {
			for _, pointer := range k.Pointers {
				if strings.HasSuffix(pointer, "*") {
					continue
				}
				ptr, found, err := s.store.Head(pointer)
				if err != nil || !found {
					continue
				}
				doc, _, err := s.store.GetBlob(ptr.Hash)
				if err != nil {
					log.Printf("seed the %s log: the %s pointer's head %s: %v", name, pointer, ptr.Hash, err)
					return
				}
				heads[k.StoreKind] = doc
				note = append(note, pointer+" "+ptr.Hash)
				hashes = append(hashes, ptr.Hash)
				if writer == "" {
					writer = "migrate:" + pointer
				}
			}
			if f, ok := files[k.StoreKind]; ok && f.path != "" && heads[k.StoreKind] == nil {
				doc, err := os.ReadFile(f.path)
				if err != nil || len(doc) == 0 {
					continue
				}
				heads[k.StoreKind] = doc
				note = append(note, "-"+f.flag+" "+store.HashOf(doc))
				hashes = append(hashes, store.HashOf(doc))
				if writer == "" {
					writer = "migrate:-" + f.flag
				}
			}
		}
		if len(heads) == 0 {
			continue
		}
		op, _ := json.Marshal(map[string]any{"cap": l.receive, "op": "migrate"})
		w := logWrite{writer: writer, op: "migrate", note: strings.Join(note, ", ")}
		// One source is the write's arguments: its hash.
		if len(hashes) == 1 {
			w.argsHash = hashes[0]
		}
		seq, _, _, refusal, err := s.receiveLog(context.Background(), nil, l, w, receiveCall{ops: []json.RawMessage{op}, heads: heads})
		if err == nil && refusal != nil {
			err = fmt.Errorf("%v", refusal.body)
		}
		if err != nil {
			log.Printf("seed the %s log: %v", name, err)
			continue
		}
		log.Printf("seed the %s log: entry %d from %s", name, seq, w.note)
	}
}
