package logs

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"net/http"
	"strconv"
	"strings"
	"time"

	"rustwasm/server/httpx"
	"rustwasm/server/store"
	"rustwasm/server/wasmhost"
)

// Landed answers whether a write with the key landed, and the hash of
// the kind's document now.
func (ls *Logs) Landed(l *Log, key, storeKind string) (landed bool, hash string, err error) {
	l.fold.mu.Lock()
	defer l.fold.mu.Unlock()
	_, landed, err = ls.keyLanded(l.fold, key)
	return landed, l.docs[storeKind].hash, err
}

type Refusal struct {
	Status int
	Body   any
}

// ReadSeq, when set, is the log's seq that the writer read. A write is
// refused (409) when it changes a record that a later entry changed.
// ClaimedAtMs is the device's clock when the person made the write, 0 if
// the writer gives none. It is a claim beside the server's stamp.
type Write struct {
	Writer, Op, ArgsHash, Key, Note string
	ReadSeq                         *int64
	// ReadSeqs holds the seqs of the other logs that the writer read
	// (the manifest's log_seqs, D-233). A log not named was read now.
	ReadSeqs    map[string]int64
	ClaimedAtMs int64
}

type logOverlap struct {
	Kind string `json:"kind"`
	ID   string `json:"id"`
	Seq  int64  `json:"seq"`
}

// keyLanded composes with nothing to write, and cap-log answers the
// entry that holds the idempotency key. The caller holds f.mu.
func (ls *Logs) keyLanded(f *fold, key string) (int64, bool, error) {
	header := map[string]any{"log": f.name, "seq": f.seq + 1, "writer": "-", "at_ms": 0, "module": "-", "intent": map[string]any{"op": "-", "key": key}}
	req := map[string]any{"op": "compose", "header": header, "next": []json.RawMessage{}}
	if len(f.base) > 0 {
		req["checkpoint"] = f.base
	}
	var composed struct {
		Landed *int64 `json:"landed"`
	}
	if err := ls.logOp(req, &composed); err != nil || composed.Landed == nil {
		return 0, false, err
	}
	return *composed.Landed, true, nil
}

// WriteFailed answers 503 for an unavailable bucket or a gap, because
// nothing was written and the client can resend. Other errors use code.
func WriteFailed(w http.ResponseWriter, err error, code int) {
	var ge *store.GapError
	switch {
	case errors.Is(err, store.ErrUnavailable):
		httpx.Fail(w, err, http.StatusServiceUnavailable)
	case errors.As(err, &ge):
		w.Header().Set("Retry-After", strconv.Itoa(httpx.RetryAfterSecs))
		http.Error(w, err.Error(), http.StatusServiceUnavailable)
	default:
		httpx.Fail(w, err, code)
	}
}

// referenceFolds answers what a write's receive op judges its references
// over (D-233). read holds each log's records as of the seq that the
// writer read, and only when judged (a person's write). A log with no
// entries is left out. The caller holds the lock of f, the write's own
// log, so the other logs come from the store and no other lock is taken.
// kinds is nil when no table exists or a log cannot be read, and then
// nothing is refused.
func (ls *Logs) referenceFolds(f *fold, w Write, judged bool) (kinds json.RawMessage, read, now map[string]json.RawMessage) {
	t, err := ls.table()
	if err != nil || t.Raw == nil {
		return nil, nil, nil
	}
	records := func(checkpoint json.RawMessage) json.RawMessage {
		var c struct {
			Records json.RawMessage `json:"records"`
		}
		json.Unmarshal(checkpoint, &c)
		if c.Records == nil {
			return json.RawMessage("{}")
		}
		return c.Records
	}
	now = map[string]json.RawMessage{}
	if judged {
		read = map[string]json.RawMessage{}
	}
	for name := range ls.all() {
		head := f.seq
		if name != f.name {
			h, err := ls.store.LogHeadOf(name)
			if err != nil {
				log.Printf("references: the %s log's head: %v", name, err)
				return nil, nil, nil
			}
			head = h.Seq
		}
		if head == 0 {
			continue
		}
		var headRecords json.RawMessage
		if name == f.name {
			headRecords = records(f.base)
		} else if headRecords, err = ls.foldAt(name, head); err != nil {
			log.Printf("references: the %s log: %v", name, err)
			return nil, nil, nil
		}
		now[name] = headRecords
		if !judged {
			continue
		}
		at, ok := w.ReadSeqs[name]
		if name == f.name {
			ok = w.ReadSeq != nil
			if ok {
				at = *w.ReadSeq
			}
		}
		switch {
		case !ok || at >= head:
			read[name] = headRecords
		case at > 0:
			if read[name], err = ls.foldAt(name, at); err != nil {
				log.Printf("references: the %s log as of %d: %v", name, at, err)
				return nil, nil, nil
			}
		}
	}
	return t.Raw, read, now
}

// ErrMoved is a write that found its log's head moved on every attempt.
// A caller finds it with errors.Is, never by its text.
var ErrMoved = errors.New("moved on every attempt")

type Received struct {
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
		Op    int                  `json:"op"`
		Error wasmhost.ModuleError `json:"error"`
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

// LoadError marks a document that the module refused when the host
// registered it. The caller's input is wrong, not the server.
type LoadError struct{ Err error }

func (e *LoadError) Error() string { return e.Err.Error() }

func (e *LoadError) Unwrap() error { return e.Err }

// A kind with a kernel load is registered, and the op gets its hash in
// `heads`. A kind with none is given inline, in `documents`.
type Call struct {
	Ops    []json.RawMessage
	Loads  []Load
	Heads  map[string][]byte
	OnLoad func(answer map[string]any)
	// OpsAfterLoads, when set, replaces Ops after the loads answer, for an
	// op argument that reads a load's answer.
	OpsAfterLoads func() []json.RawMessage
}

type Load struct {
	Code uint32
	Doc  []byte
}

// Receive creates one entry from one write, asking again over the new
// head when another write took the seq (D-190, rule 2). `r` is nil when
// the server makes the write itself or the route checked the write rule
// already.
func (ls *Logs) Receive(ctx context.Context, r *http.Request, l *Log, w Write, call Call) (seq int64, landed bool, got Received, refusal *Refusal, err error) {
	f := l.fold
	f.mu.Lock()
	defer f.mu.Unlock()
	for attempt := 0; attempt < 5; attempt++ {
		if err := ls.refreshLogLocked(f); err != nil {
			return 0, false, got, nil, err
		}
		if f.stale {
			return 0, false, got, nil, errStaleHead(f)
		}
		if len(f.gaps) > 0 {
			return 0, false, got, nil, &store.GapError{Log: f.name, Gaps: f.gaps}
		}
		intent := map[string]any{"op": w.Op}
		for k, v := range map[string]string{"args_hash": w.ArgsHash, "key": w.Key, "note": w.Note} {
			if v != "" {
				intent[k] = v
			}
		}
		if w.ClaimedAtMs > 0 {
			intent["claimed_at_ms"] = w.ClaimedAtMs
		}
		write := map[string]any{"header": map[string]any{"log": f.name, "seq": f.seq + 1, "writer": w.Writer, "at_ms": time.Now().UnixMilli(), "intent": intent}}
		// The write's date in the agency's zone is the date that a reference
		// rule reads a dated state on.
		write["today"] = ls.writeDate()
		if len(f.base) > 0 {
			write["checkpoint"] = f.base
		}
		if w.ReadSeq != nil && *w.ReadSeq < f.seq {
			unread, err := ls.store.Entries(f.name, max(*w.ReadSeq, 0), f.seq)
			if err != nil {
				return 0, false, got, nil, err
			}
			write["unread"] = unread
		}
		// The reference checks (D-233) read each log as of the seq the writer
		// read, for a person's write (the server's own writes are never refused
		// for a reference), and each log now, for the warnings.
		if kinds, read, now := ls.referenceFolds(f, w, r != nil); kinds != nil {
			write["references"], write["now"] = kinds, now
			if read != nil {
				write["read"] = read
			}
		}
		got = Received{}
		long := ls.long()
		err := long.Sequence(ctx, func(inst *wasmhost.Instance) error {
			host, declares := inst.As(l.receive), inst.Documents(l.receive)
			write["header"].(map[string]any)["module"] = "cap-" + l.receive + "@" + host.Hash()
			loadedHashes := map[string]string{}
			for _, load := range call.Loads {
				var loaded map[string]any
				if err := host.InvokeBytes(ctx, load.Code, load.Doc, &loaded); err != nil {
					return &LoadError{err}
				}
				for k, v := range loaded {
					if h, ok := v.(string); ok && strings.HasSuffix(k, "_hash") {
						loadedHashes[k] = h
					}
				}
				if call.OnLoad != nil {
					call.OnLoad(loaded)
				}
			}
			heads := map[string]string{}
			documents := map[string]json.RawMessage{}
			for _, k := range l.kinds {
				doc := l.docs[k.StoreKind].doc
				if call.Heads != nil {
					doc = call.Heads[k.StoreKind]
				}
				if len(doc) == 0 {
					continue
				}
				if k.Load == nil || !declares[strings.TrimPrefix(k.Load.Op, "load_")] {
					documents[k.StoreKind] = json.RawMessage(doc)
					continue
				}
				if call.Heads != nil {
					var loaded map[string]any
					if err := host.InvokeBytes(ctx, k.Load.Code, doc, &loaded); err != nil {
						return fmt.Errorf("the %s document at the head: %w", k.StoreKind, err)
					}
					heads[k.StoreKind] = wasmhost.LoadedHash(loaded)
					continue
				}
				hash, err := long.Head(ctx, inst, l.name+"/"+k.StoreKind, l.docs[k.StoreKind].hash, k.Load.Code, doc)
				if err != nil {
					return fmt.Errorf("the %s document at the head: %w", k.StoreKind, err)
				}
				heads[k.StoreKind] = hash
			}
			ops := call.Ops
			if call.OpsAfterLoads != nil {
				ops = call.OpsAfterLoads()
			}
			req := map[string]any{"op": "receive", "write": write, "heads": heads, "ops": ops}
			if len(documents) > 0 {
				req["documents"] = documents
			}
			if len(loadedHashes) > 0 {
				req["loaded"] = loadedHashes
			}
			if r != nil {
				req["by"] = httpx.Name(r, "anonymous")
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
			return 0, false, got, &Refusal{http.StatusBadRequest, map[string]any{"op": got.Failed.Op, "error": got.Failed.Error}}, nil
		case len(got.Broken) > 0:
			return 0, false, got, &Refusal{http.StatusUnprocessableEntity, map[string]any{"refusals": []any{}, "references": got.Broken}}, nil
		case got.Refused != nil:
			return 0, false, got, &Refusal{http.StatusUnprocessableEntity, map[string]any{"op": got.Refused.Op, "refusals": got.Refused.Refusals}}, nil
		case got.Moved != nil:
			return 0, false, got, &Refusal{http.StatusConflict, map[string]any{"current": got.Moved.Current, "seq": f.seq}}, nil
		case got.Conflict != nil:
			return 0, false, got, &Refusal{http.StatusConflict, map[string]any{"op": got.Conflict.Op, "conflict": got.Conflict.ID}}, nil
		case got.Overlap != nil:
			o := got.Overlap
			return 0, false, got, &Refusal{http.StatusConflict, map[string]any{"conflict": o.Kind + " " + o.ID, "overlap": o, "seq": f.seq}}, nil
		}
		for kind := range got.Heads {
			if r == nil {
				break
			}
			if allow, reason := ls.mayWrite(r, l.ruleOf(kind)); !allow {
				return 0, false, got, &Refusal{http.StatusForbidden, "forbidden: " + reason}, nil
			}
		}
		if got.Entry == "" {
			return f.seq, false, got, nil, nil
		}
		module := write["header"].(map[string]any)["module"].(string)
		for _, d := range got.Made {
			env := store.Envelope{Kind: d.Kind, SchemaVersion: d.Version, ProducedBy: module}
			if d.Pointer == "" {
				if _, err := ls.store.PutBlob([]byte(d.Document), env, d.DocumentHash); err != nil {
					return 0, false, got, nil, err
				}
				continue
			}
			if !store.ValidPointerName(d.Pointer) {
				return 0, false, got, &Refusal{http.StatusBadRequest, map[string]any{"error": fmt.Sprintf("%q cannot name a pointer (letters, digits, '.', '_', '-')", d.Pointer)}}, nil
			}
			if err := ls.store.CommitHead(d.Pointer, []byte(d.Document), env, d.DocumentHash, w.Writer); err != nil {
				return 0, false, got, nil, err
			}
		}
		err = ls.store.AppendEntry(f.name, f.seq+1, []byte(got.Entry))
		var ce *store.EntryConflictError
		if errors.As(err, &ce) {
			continue
		}
		if err != nil {
			return 0, false, got, nil, err
		}
		if err := ls.refreshLogLocked(f); err != nil {
			return 0, false, got, nil, err
		}
		return f.seq, false, got, nil, nil
	}
	return 0, false, got, nil, fmt.Errorf("the %s log %w", f.name, ErrMoved)
}

func WriteRefused(w http.ResponseWriter, refusal *Refusal) {
	if text, ok := refusal.Body.(string); ok {
		http.Error(w, text, refusal.Status)
		return
	}
	httpx.JSON(w, refusal.Status, refusal.Body)
}
