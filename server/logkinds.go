package main

// A log whose kinds name a receiving capability is written through its
// `receive` op (D-228; docs/architecture/server.md, "One path for a log").
// The host decides nothing: the op answers the entry and the documents it
// wrote.
//
// A key that landed answers the entry that holds it, so a resend lands
// once. A write is a 409 only when an entry after the page's `read_seq`
// changed a record that the write changes (D-189).

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
	"time"

	"rustwasm/server/store"
)

type logDoc struct {
	hash string
	doc  []byte
}

// docs is guarded by fold.mu.
type keptLog struct {
	name    string
	receive string
	kinds   []kindEntry
	fold    *logFold
	docs    map[string]logDoc
	intent  bool
}

// withModule runs `use` as the capability on the long-lived instance.
// Only the logs' heads outlive the call (wasm.go, `sequence`).
func (s *server) withModule(ctx context.Context, capability string, use func(*WasmHost) error) error {
	return s.longLived().sequence(ctx, func(inst *Instance) error { return use(inst.As(capability)) })
}

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
					if len(l.kinds) == 1 {
						hashes["document"] = l.docs[l.kinds[0].StoreKind].hash
					}
					return hashes
				}}
				s.kept[e.Log] = l
			}
			l.kinds = append(l.kinds, e)
			// A revert of an LWW kind writes its records back, stamped to win the
			// merge (D-193).
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

func errNoLog(name string) error {
	return fmt.Errorf("the kind table names no %s log (run `just caps`)", name)
}

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

func (s *server) keptLog(name string) *keptLog {
	return s.keptLogs()[name]
}

func (s *server) keptNames() []string {
	var names []string
	for name := range s.keptLogs() {
		names = append(names, name)
	}
	sort.Strings(names)
	return names
}

// Caller holds the fold's lock.
func (s *server) assembleKept(l *keptLog, records map[string]json.RawMessage) error {
	ctx := context.Background()
	l.docs = map[string]logDoc{}
	for _, k := range l.kinds {
		if len(k.Records) == 0 || k.Module == "" {
			continue
		}
		// A kind whose merge is LWW always has a document: an empty log's is the
		// empty document.
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
		// The long-lived instance holds each head from the moment it is
		// assembled, so a write registers none, and a refresh carries them.
		long := s.longLived()
		err := long.sequence(ctx, func(inst *Instance) error {
			if err := inst.As(k.Module).Invoke(ctx, req, &out); err != nil {
				return err
			}
			if k.Load == nil || !inst.Documents(l.receive)[strings.TrimPrefix(k.Load.Op, "load_")] {
				return nil
			}
			_, err := long.head(ctx, inst, l.name+"/"+k.StoreKind, out.DocumentHash, k.Load.Code, []byte(out.Document))
			return err
		})
		if err != nil {
			return fmt.Errorf("the %s document: %w", k.StoreKind, err)
		}
		l.docs[k.StoreKind] = logDoc{out.DocumentHash, []byte(out.Document)}
	}
	return nil
}

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

func ownRecord(records map[string]json.RawMessage, kind string) json.RawMessage {
	var byID map[string]json.RawMessage
	if raw, ok := records[kind]; ok {
		json.Unmarshal(raw, &byID)
	}
	return byID["document"]
}

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

func (l *keptLog) ruleOf(storeKind string) string {
	for _, k := range l.kinds {
		if k.StoreKind == storeKind {
			return k.rule()
		}
	}
	return storeKind
}

func (l *keptLog) loadCode(storeKind string) uint32 {
	for _, k := range l.kinds {
		if k.StoreKind == storeKind && k.Load != nil {
			return k.Load.Code
		}
	}
	return 0
}

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

type received struct {
	Entry    string      `json:"entry"`
	Changes  int         `json:"changes"`
	Landed   *int64      `json:"landed"`
	Overlap  *logOverlap `json:"overlap"`
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
	// Broken references refuse the write (D-233). References are warnings.
	Broken     []json.RawMessage `json:"broken"`
	References []json.RawMessage `json:"references"`
	// Made documents are stored before the entry, so the entry never names a
	// document that the store lacks.
	Made   []madeDocument  `json:"made"`
	Answer json.RawMessage `json:"answer"`
}

// Pointer is empty for a document stored as a blob only.
type madeDocument struct {
	Kind         string `json:"kind"`
	Version      uint32 `json:"version"`
	Document     string `json:"document"`
	DocumentHash string `json:"document_hash"`
	Pointer      string `json:"pointer"`
}

// loadError marks a document that the module refused when the host
// registered it. The caller's input is wrong, not the server.
type loadError struct{ err error }

func (e *loadError) Error() string { return e.err.Error() }
func (e *loadError) Unwrap() error { return e.err }

// A kind with a kernel load is registered, and the op gets its hash in
// `heads`. A kind with none is given inline, in `documents`.
type receiveCall struct {
	ops    []json.RawMessage
	loads  []hostLoad
	heads  map[string][]byte
	onLoad func(answer map[string]any)
	// opsAfterLoads, when set, replaces ops after the loads answer, for an op
	// argument that reads a load's answer.
	opsAfterLoads func() []json.RawMessage
}

type hostLoad struct {
	code uint32
	doc  []byte
}

func loadedHash(answer map[string]any) string {
	for k, v := range answer {
		if h, ok := v.(string); ok && strings.HasSuffix(k, "_hash") {
			return h
		}
	}
	return ""
}

// receiveLog creates one entry from one write, asking again over the new
// head when another write took the seq (D-190, rule 2). `r` is nil when
// the server makes the write itself or the route checked the write rule
// already.
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
		// The write's date in the agency's zone is the date that a reference
		// rule reads a dated state on (D-238).
		write["today"] = s.writeDate()
		if len(f.base) > 0 {
			write["checkpoint"] = f.base
		}
		if w.readSeq != nil && *w.readSeq < f.seq {
			unread, err := s.store.Entries(f.name, max(*w.readSeq, 0), f.seq)
			if err != nil {
				return 0, false, got, nil, err
			}
			write["unread"] = unread
		}
		// The reference checks (D-233) read each log as of the seq the writer
		// read, for a person's write (the server's own writes are never refused
		// for a reference), and each log now, for the warnings.
		if kinds, read, now := s.referenceFolds(f, w, r != nil); kinds != nil {
			write["references"], write["now"] = kinds, now
			if read != nil {
				write["read"] = read
			}
		}
		got = received{}
		long := s.longLived()
		err := long.sequence(ctx, func(inst *Instance) error {
			host, declares := inst.As(l.receive), inst.Documents(l.receive)
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
				if k.Load == nil || !declares[strings.TrimPrefix(k.Load.Op, "load_")] {
					documents[k.StoreKind] = json.RawMessage(doc)
					continue
				}
				if call.heads != nil {
					var loaded map[string]any
					if err := host.InvokeBytes(ctx, k.Load.Code, doc, &loaded); err != nil {
						return fmt.Errorf("the %s document at the head: %w", k.StoreKind, err)
					}
					heads[k.StoreKind] = loadedHash(loaded)
					continue
				}
				hash, err := long.head(ctx, inst, l.name+"/"+k.StoreKind, l.docs[k.StoreKind].hash, k.Load.Code, doc)
				if err != nil {
					return fmt.Errorf("the %s document at the head: %w", k.StoreKind, err)
				}
				heads[k.StoreKind] = hash
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
		case len(got.Broken) > 0:
			return 0, false, got, &logRefusal{http.StatusUnprocessableEntity, map[string]any{"refusals": []any{}, "references": got.Broken}}, nil
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

func writeRefused(w http.ResponseWriter, refusal *logRefusal) {
	if text, ok := refusal.body.(string); ok {
		http.Error(w, text, refusal.status)
		return
	}
	writeJSON(w, refusal.status, refusal.body)
}

func (s *server) handleKeptIntent(w http.ResponseWriter, r *http.Request, l *keptLog) {
	body, err := io.ReadAll(io.LimitReader(r.Body, 32<<20))
	var req struct {
		Key         string            `json:"key"`
		Ops         []json.RawMessage `json:"ops"`
		ReadSeq     *int64            `json:"read_seq"`
		ReadSeqs    map[string]int64  `json:"read_seqs"`
		ClaimedAtMs int64             `json:"claimed_at_ms"`
	}
	if err == nil {
		err = json.Unmarshal(body, &req)
	}
	if err != nil || len(req.Ops) == 0 {
		http.Error(w, "the "+l.name+` log's write: body: {"key": "<idempotency key>", "ops": [{"cap": "...", "op": "...", "args": {...}}], "read_seq": N, "read_seqs": {"<log>": N}, "claimed_at_ms": T}`, http.StatusBadRequest)
		return
	}
	write := logWrite{writer: writerOf(r, "anonymous"), op: "POST /logs/" + l.name, argsHash: store.HashOf(body), key: req.Key, readSeq: req.ReadSeq, readSeqs: req.ReadSeqs, claimedAtMs: req.ClaimedAtMs}
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
	} else if len(got.References) > 0 {
		answer["references"] = got.References
	}
	writeJSON(w, http.StatusOK, answer)
}

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

func (s *server) handleKeptRoute(w http.ResponseWriter, r *http.Request) {
	if s.store == nil {
		http.Error(w, r.URL.Path+" needs the store (start with -store <url>)", http.StatusNotFound)
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
	_, landed, got, refusal, err := s.receiveLog(r.Context(), r, route.log, write, receiveCall{ops: []json.RawMessage{op}})
	if err != nil {
		logWriteFailed(w, err, http.StatusBadRequest)
		return
	}
	if refusal != nil {
		// An op that did not run is the request's 400.
		if got.Failed != nil {
			http.Error(w, k.StoreKind+": body: "+got.Failed.Error, http.StatusBadRequest)
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
	if !landed && len(got.References) > 0 {
		answer["references"] = got.References
	}
	writeJSON(w, http.StatusOK, answer)
}

func (s *server) keptRouteRule(r *http.Request) string {
	if route, ok := s.keptRoutes()[r.URL.Path]; ok {
		return route.kind.rule()
	}
	return strings.TrimPrefix(r.URL.Path, "/")
}

type seedFile struct {
	flag, path string
}

// seedKept gives each empty log its first entry at startup, from its
// kinds' old pointers, else a file in `files` (the dev store's -edits).
// The writer is migrate:<the first pointer read>, or migrate:-<flag>.
// The pointers are never written again.
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
		for kind, doc := range heads {
			if fwd, migrated, err := s.forward(kind, doc); err == nil && migrated {
				heads[kind] = fwd
			}
		}
		op, _ := json.Marshal(map[string]any{"cap": l.receive, "op": "migrate"})
		w := logWrite{writer: writer, op: "migrate", note: strings.Join(note, ", ")}
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
