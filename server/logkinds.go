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
//	POST <a kind's own route> (the kind table's `post`): one op over the body
//	                  200 {<answer>: the document after the write, <echo>...}
//	                  422 {op, refusals}   409 {current, seq} | {conflict, overlap, seq}
//
// The manifest pins each kind's document as `<store kind>_hash`, and
// /blobs serves it by that hash.

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"sort"
	"strings"
	"sync"
	"time"

	"rustwasm/server/store"
)

// keptLog is a log a receive op writes: its fold, the kinds it holds (in
// the kind table's order), the capability whose receive op writes it,
// and each kind's document at the head, by store kind (fold.mu held).
type keptLog struct {
	name    string
	receive string
	kinds   []kindEntry
	fold    *logFold
	docs    map[string]logDoc
}

// heldModule is the server's one instance of a capability, made on first
// use.
type heldModule struct {
	mu   sync.Mutex
	host *WasmHost
}

// modulePath is where a capability's module is: the one the server was
// started with (-<cap>-wasm), or where `just caps` writes it.
func (s *server) modulePath(capability string) string {
	if p := s.checkWasm[capability]; p != "" {
		return p
	}
	if p := s.serverModules[capability]; p != "" {
		return p
	}
	return "../web/pkg/cap_" + capability + ".wasm"
}

// withModule runs `use` on the server's one instance of a capability,
// then empties it (the kernel's evict), so no document outlives the
// request that sent it.
func (s *server) withModule(ctx context.Context, capability string, use func(*WasmHost) error) error {
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
		m.host = host
	}
	err := use(m.host)
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
				l.fold = &logFold{name: e.Log, assemble: func(records map[string]json.RawMessage) error { return s.assembleKept(l, records) }}
				s.kept[e.Log] = l
			}
			l.kinds = append(l.kinds, e)
		}
	})
	return s.kept
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
		raw, ok := records[k.Records[0].Kind]
		if !ok || len(raw) <= 2 {
			continue
		}
		req := map[string]any{"op": "log_document", "kind": k.StoreKind, "records": raw}
		if own := ownRecord(records, k.StoreKind); own != nil {
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

// received is a receive op's answer.
type received struct {
	Entry   string      `json:"entry"`
	Landed  *int64      `json:"landed"`
	Overlap *logOverlap `json:"overlap"`
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
}

// receiveLog runs one write to a log through its receive op into one
// entry: the documents at the head registered in the receiving module,
// one call, the write rule of each document it wrote, and the entry
// created at the next seq, asked again over the new head when another
// write took it (D-190, rule 2). The answer is the entry's seq (the
// head's when nothing changed), and landed when the write's key landed
// before.
func (s *server) receiveLog(r *http.Request, l *keptLog, w logWrite, ops []json.RawMessage) (seq int64, landed bool, got received, refusal *logRefusal, err error) {
	f := l.fold
	f.mu.Lock()
	defer f.mu.Unlock()
	ctx := r.Context()
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
		err := s.withModule(ctx, l.receive, func(host *WasmHost) error {
			write["header"].(map[string]any)["module"] = "cap-" + l.receive + "@" + host.Hash()
			heads := map[string]string{}
			for _, k := range l.kinds {
				held := l.docs[k.StoreKind]
				if held.hash == "" || k.Load == nil {
					continue
				}
				var loaded map[string]any
				if err := host.InvokeBytes(ctx, k.Load.Code, held.doc, &loaded); err != nil {
					return fmt.Errorf("the %s document at the head: %w", k.StoreKind, err)
				}
				heads[k.StoreKind] = held.hash
			}
			return host.Invoke(ctx, map[string]any{"op": "receive", "write": write, "heads": heads, "ops": ops}, &got)
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
		case got.Overlap != nil:
			o := got.Overlap
			return 0, false, got, &logRefusal{http.StatusConflict, map[string]any{"conflict": o.Kind + " " + o.ID, "overlap": o, "seq": f.seq}}, nil
		}
		// The write rule of each document the write's ops wrote.
		for kind := range got.Heads {
			if allow, reason := s.mayWrite(r, kind); !allow {
				return 0, false, got, &logRefusal{http.StatusForbidden, "forbidden: " + reason}, nil
			}
		}
		if got.Entry == "" {
			return f.seq, false, got, nil, nil
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
	seq, landed, got, refusal, err := s.receiveLog(r, l, write, req.Ops)
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
// arguments of the route's op, run by its log's receive op.
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
		http.Error(w, "body: a JSON object, the arguments of "+route.log.receive+" "+post.Op, http.StatusBadRequest)
		return
	}
	op, _ := json.Marshal(map[string]any{"cap": route.log.receive, "op": post.Op, "args": args})
	write := logWrite{writer: writerOf(r, "anonymous"), op: "POST " + post.Path, argsHash: store.HashOf(body)}
	_, _, got, refusal, err := s.receiveLog(r, route.log, write, []json.RawMessage{op})
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
	writeJSON(w, http.StatusOK, answer)
}

// keptRouteRule is the write rule of a kind's own route: its store kind.
func (s *server) keptRouteRule(r *http.Request) string {
	if route, ok := s.keptRoutes()[r.URL.Path]; ok {
		return route.kind.StoreKind
	}
	return strings.TrimPrefix(r.URL.Path, "/")
}
