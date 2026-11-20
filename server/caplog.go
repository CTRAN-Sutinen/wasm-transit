package main

// A log that the log capability keeps (cap-log; D-190, D-197;
// docs/FORMATS.md, "The logs"): the host's half. The bucket holds the
// entries and checkpoints (store/log.go); cap-log answers each write's
// entry, the fold, and a revert, and never reads a record. Each kind's
// capability turns the fold's records back into its document
// (`assemble`). This host only orders and keeps the entries, and treats
// every entry, checkpoint, record, and document as opaque bytes.
//
// One logFold per log: the disruptions log (disruptions.go), the stops
// log (stopslog.go), and the plans log (planslog.go).

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net/http"
	"sort"
	"strconv"
	"sync"
	"time"

	"rustwasm/server/store"
)

// defaultLogWasm is where `just caps` puts the log module.
const defaultLogWasm = "../web/pkg/cap_log.wasm"

// checkpointEvery is how many entries the server folds past a log's
// newest checkpoint before it stores the next one. A boot reads the
// newest checkpoint and folds at most this many entries after it.
const checkpointEvery = 256

// logEntriesLimit is the most entries one GET /logs/<log> answers.
const logEntriesLimit = 1000

// logModule is the hosted log capability (cap-log), loaded on first
// use and held for the server's lifetime.
func (s *server) logModule() (*WasmHost, error) {
	s.logOnce.Do(func() {
		path := s.logWasm
		if path == "" {
			path = defaultLogWasm
		}
		s.logHost, s.logErr = NewWasmHost(context.Background(), path)
	})
	return s.logHost, s.logErr
}

func (s *server) logOp(req map[string]any, out any) error {
	m, err := s.logModule()
	if err != nil {
		return fmt.Errorf("the log module: %w", err)
	}
	return m.Invoke(context.Background(), req, out)
}

// logFold is this process's fold of one log.
type logFold struct {
	name string
	// assemble turns the fold's records (by kind, then by id) into the
	// log's documents, through each kind's capability.
	assemble func(records map[string]json.RawMessage) error
	// revise, when set, answers the records a revert writes back in
	// place of the states before (cap-log `written`), through the kind's
	// capability: a kind whose merge would undo the revert (D-193).
	revise func(states json.RawMessage, nowMs int64) (json.RawMessage, error)
	// documents, when set, names the hashes of the documents the fold
	// serves, for GET /logs/<log>. Called with mu held.
	documents func() map[string]string

	mu     sync.Mutex
	loaded bool
	// base is the checkpoint at seq, as cap-log answered it.
	base json.RawMessage
	seq  int64
	// stored is the seq of the newest checkpoint in the bucket.
	stored      int64
	asOfMs      int64
	quarantined map[int64]string
	gaps        []int64
}

// foldAnswer is cap-log fold's answer.
type logFoldAnswer struct {
	Seq         int64                      `json:"seq"`
	Checkpoint  string                     `json:"checkpoint"`
	Records     map[string]json.RawMessage `json:"records"`
	Quarantined []struct {
		Seq    int64  `json:"seq"`
		Reason string `json:"reason"`
	} `json:"quarantined"`
}

func (s *server) foldLog(name string, checkpoint json.RawMessage, entries []store.StoredEntry) (logFoldAnswer, error) {
	if entries == nil {
		entries = []store.StoredEntry{}
	}
	req := map[string]any{"op": "fold", "log": name, "entries": entries}
	if len(checkpoint) > 0 {
		req["checkpoint"] = checkpoint
	}
	var ans logFoldAnswer
	err := s.logOp(req, &ans)
	return ans, err
}

// bootLocked picks the checkpoint a fresh process starts from: the
// newest one that equals the fold from the checkpoint before it (or
// from the empty log). One that differs raises an alarm. Caller holds
// f.mu.
func (s *server) bootLogLocked(f *logFold, upto int64) error {
	cps, err := s.store.Checkpoints(f.name)
	if err != nil {
		return err
	}
	f.base, f.seq, f.stored = nil, 0, 0
	for i := len(cps) - 1; i >= 0; i-- {
		at := cps[i]
		if at > upto {
			continue
		}
		var prev json.RawMessage
		var prevSeq int64
		if i > 0 {
			prevSeq = cps[i-1]
			if prev, err = s.store.Checkpoint(f.name, prevSeq); err != nil {
				return err
			}
		}
		stored, err := s.store.Checkpoint(f.name, at)
		if err != nil {
			return err
		}
		entries, err := s.store.Entries(f.name, prevSeq, at)
		if err != nil {
			return err
		}
		ans, err := s.foldLog(f.name, prev, entries)
		if err == nil && ans.Seq == at && ans.Checkpoint == string(stored) {
			f.base, f.seq, f.stored = json.RawMessage(stored), at, at
			return nil
		}
		log.Printf("ALARM %s log: checkpoint %d differs from the fold of the entries before it (%v): the boot starts from an older one", f.name, at, err)
	}
	return nil
}

// refreshLocked brings the fold to the log's head: the entries up to
// the first gap, folded from the last fold, and the documents assembled
// again when anything moved. It stores a checkpoint every
// checkpointEvery entries. Caller holds f.mu.
func (s *server) refreshLogLocked(f *logFold) error {
	head, err := s.store.LogHeadOf(f.name)
	if err != nil {
		return err
	}
	upto := head.Seq
	if len(head.Gaps) > 0 {
		upto = head.Gaps[0] - 1
		if len(f.gaps) == 0 || f.gaps[0] != head.Gaps[0] {
			log.Printf("ALARM %s log: a gap at %v: writes stop until POST /logs/%s/close-gap names the cause", f.name, head.Gaps, f.name)
		}
	}
	f.gaps = head.Gaps
	if !f.loaded {
		if err := s.bootLogLocked(f, upto); err != nil {
			return err
		}
		f.quarantined = map[int64]string{}
	}
	if upto > f.seq || !f.loaded {
		entries, err := s.store.Entries(f.name, f.seq, upto)
		if err != nil {
			return err
		}
		ans, err := s.foldLog(f.name, f.base, entries)
		if err != nil {
			return fmt.Errorf("fold the %s log: %w", f.name, err)
		}
		for _, q := range ans.Quarantined {
			if _, seen := f.quarantined[q.Seq]; !seen {
				log.Printf("ALARM %s log: entry %d is quarantined: %s", f.name, q.Seq, q.Reason)
			}
			f.quarantined[q.Seq] = q.Reason
		}
		if err := f.assemble(ans.Records); err != nil {
			return fmt.Errorf("the %s log's documents: %w", f.name, err)
		}
		f.base, f.seq = json.RawMessage(ans.Checkpoint), ans.Seq
		f.loaded = true
		if f.seq-f.stored >= checkpointEvery {
			if err := s.store.PutCheckpoint(f.name, f.seq, f.base); err != nil {
				log.Printf("ALARM %s log: checkpoint %d: %v", f.name, f.seq, err)
			} else {
				f.stored = f.seq
			}
		}
	}
	f.asOfMs = head.AsOfMs
	return nil
}

// refreshLog brings the fold to the head. While the bucket does not
// answer, a fold this process holds stays, stale (D-119): stale reports
// that.
func (s *server) refreshLog(f *logFold) (stale bool, err error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	err = s.refreshLogLocked(f)
	if err != nil && errors.Is(err, store.ErrUnavailable) && f.loaded {
		return true, nil
	}
	return false, err
}

// logRefusal is a write the caller refuses before an entry: the status
// and the body to answer.
type logRefusal struct {
	status int
	body   any
}

// logWrite is one write to a log: who, what asked for it, and the
// writer's idempotency key. readSeq, when set, is the log's seq the
// writer read: a record the write changes that a later entry changed
// is refused (409), and a change to another record is not. claimedAtMs
// is the device's clock when the person made the write: a claim beside
// the server's stamp (D-222), 0 when the writer gives none.
type logWrite struct {
	writer, op, argsHash, key, note string
	readSeq                         *int64
	claimedAtMs                     int64
}

// logOverlap is the 409 of a write that changes a record an entry
// after its read changed too: a person settles it.
type logOverlap struct {
	Kind string `json:"kind"`
	ID   string `json:"id"`
	Seq  int64  `json:"seq"`
}

// writeLog runs one write through the receive path into one entry.
// `run` answers, over the documents at the head, the next records of
// each kind the write touches (cap-log's `next`), or a refusal. cap-log
// composes the entry; a lost race reads the log again and runs `run`
// again (D-190, rule 2). The answer is the entry's seq (the head's when
// nothing changed), and landed when the write's key landed before.
func (s *server) writeLog(f *logFold, w logWrite, run func() ([]json.RawMessage, *logRefusal, error)) (seq int64, landed bool, refusal *logRefusal, err error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	m, err := s.logModule()
	if err != nil {
		return 0, false, nil, fmt.Errorf("the log module: %w", err)
	}
	for attempt := 0; attempt < 5; attempt++ {
		if err := s.refreshLogLocked(f); err != nil {
			return 0, false, nil, err
		}
		if len(f.gaps) > 0 {
			return 0, false, nil, &store.GapError{Log: f.name, Gaps: f.gaps}
		}
		// A key that landed answers before the write runs again: its ops
		// may no longer pass over the head they changed.
		if w.key != "" {
			if seq, ok, err := s.keyLanded(f, w.key); err != nil || ok {
				return seq, ok, nil, err
			}
		}
		next, refusal, err := run()
		if err != nil || refusal != nil {
			return 0, false, refusal, err
		}
		if next == nil {
			next = []json.RawMessage{}
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
		header := map[string]any{"log": f.name, "seq": f.seq + 1, "writer": w.writer, "at_ms": time.Now().UnixMilli(), "module": "cap-log@" + m.Hash(), "intent": intent}
		req := map[string]any{"op": "compose", "header": header, "next": next}
		if len(f.base) > 0 {
			req["checkpoint"] = f.base
		}
		// The entries the writer did not read: a record the write
		// changes that one of them changed is for a person to settle.
		if w.readSeq != nil && *w.readSeq < f.seq {
			unread, err := s.store.Entries(f.name, max(*w.readSeq, 0), f.seq)
			if err != nil {
				return 0, false, nil, err
			}
			req["unread"] = unread
		}
		var composed struct {
			Entry   string      `json:"entry"`
			Landed  *int64      `json:"landed"`
			Overlap *logOverlap `json:"overlap"`
		}
		if err := s.logOp(req, &composed); err != nil {
			return 0, false, nil, err
		}
		if composed.Landed != nil {
			return *composed.Landed, true, nil, nil
		}
		if composed.Overlap != nil {
			o := composed.Overlap
			return 0, false, &logRefusal{http.StatusConflict, map[string]any{"conflict": o.Kind + " " + o.ID, "overlap": o, "seq": f.seq}}, nil
		}
		if composed.Entry == "" {
			return f.seq, false, nil, nil
		}
		err = s.store.AppendEntry(f.name, f.seq+1, []byte(composed.Entry))
		var ce *store.EntryConflictError
		if errors.As(err, &ce) {
			continue
		}
		if err != nil {
			return 0, false, nil, err
		}
		if err := s.refreshLogLocked(f); err != nil {
			return 0, false, nil, err
		}
		return f.seq, false, nil, nil
	}
	return 0, false, nil, fmt.Errorf("the %s log moved on every attempt", f.name)
}

// keyLanded asks cap-log whether the fold holds an idempotency key:
// compose with nothing to write answers the entry that holds it. Caller
// holds f.mu.
func (s *server) keyLanded(f *logFold, key string) (int64, bool, error) {
	header := map[string]any{"log": f.name, "seq": f.seq + 1, "writer": "-", "at_ms": 0, "module": "-", "intent": map[string]any{"op": "-", "key": key}}
	req := map[string]any{"op": "compose", "header": header, "next": []json.RawMessage{}}
	if len(f.base) > 0 {
		req["checkpoint"] = f.base
	}
	var composed struct {
		Landed *int64 `json:"landed"`
	}
	if err := s.logOp(req, &composed); err != nil || composed.Landed == nil {
		return 0, false, err
	}
	return *composed.Landed, true, nil
}

// logWriteFailed answers a write the log did not take: a bucket that does
// not answer or a gap is 503 (nothing was written; resend), a lost race
// on every attempt is 409, and anything else is the caller's code.
func logWriteFailed(w http.ResponseWriter, err error, code int) {
	var ge *store.GapError
	switch {
	case errors.Is(err, store.ErrUnavailable):
		fail(w, err, http.StatusServiceUnavailable)
	case errors.As(err, &ge):
		w.Header().Set("Retry-After", strconv.Itoa(retryAfterSecs))
		http.Error(w, err.Error(), http.StatusServiceUnavailable)
	default:
		fail(w, err, code)
	}
}

// handleLogEntriesOf answers a cap-log log's entries after `since`, at
// most `limit`, with the head, the gaps, the newest checkpoint, and the
// entries the fold quarantined.
func (s *server) handleLogEntriesOf(w http.ResponseWriter, r *http.Request, f *logFold) {
	since, err := strconv.ParseInt(r.URL.Query().Get("since"), 10, 64)
	if r.URL.Query().Get("since") == "" {
		since, err = 0, nil
	}
	if err != nil || since < 0 {
		http.Error(w, "since is an entry's seq (0 or more)", http.StatusBadRequest)
		return
	}
	limit := int64(logEntriesLimit)
	if v := r.URL.Query().Get("limit"); v != "" {
		if n, err := strconv.ParseInt(v, 10, 64); err == nil && n > 0 && n < limit {
			limit = n
		}
	}
	stale, err := s.refreshLog(f)
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return
	}
	lh, err := s.store.LogHeadOf(f.name)
	if err != nil {
		lh = store.LogHead{Log: f.name, Seq: f.seq}
	}
	stored, err := s.store.Entries(f.name, since, min(lh.Seq, since+limit))
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return
	}
	type item struct {
		Seq        int64           `json:"seq"`
		Entry      json.RawMessage `json:"entry,omitempty"`
		Unreadable string          `json:"unreadable,omitempty"`
	}
	items := make([]item, 0, len(stored))
	for _, e := range stored {
		if json.Valid([]byte(e.Text)) {
			items = append(items, item{Seq: e.Seq, Entry: json.RawMessage(e.Text)})
		} else {
			items = append(items, item{Seq: e.Seq, Unreadable: e.Text})
		}
	}
	type held struct {
		Seq    int64  `json:"seq"`
		Reason string `json:"reason"`
	}
	f.mu.Lock()
	quarantined := make([]held, 0, len(f.quarantined))
	for seq, why := range f.quarantined {
		quarantined = append(quarantined, held{seq, why})
	}
	asOf := f.asOfMs
	answer := map[string]any{
		"log":         f.name,
		"head":        lh.Seq,
		"gaps":        lh.Gaps,
		"checkpoint":  lh.Checkpoint,
		"entries":     items,
		"quarantined": quarantined,
	}
	if f.documents != nil {
		for k, v := range f.documents() {
			answer[k] = v
		}
	}
	f.mu.Unlock()
	sort.Slice(quarantined, func(i, j int) bool { return quarantined[i].Seq < quarantined[j].Seq })
	labelStale(w, store.Head{Stale: stale, AsOfMs: asOf})
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Cache-Control", "no-cache")
	json.NewEncoder(w).Encode(answer)
}

// handleLogRevertOf writes the entry that reverts entry `seq` of a
// cap-log log. cap-log refuses it, naming the record, when a later entry
// changed one of the records: that answers 409, because a person
// decides. A log whose kind writes back its own records (f.revise)
// asks it over cap-log's states, then asks cap-log for the entry again
// with them.
func (s *server) handleLogRevertOf(w http.ResponseWriter, r *http.Request, f *logFold) {
	var req struct {
		Seq int64 `json:"seq"`
	}
	if err := json.NewDecoder(io.LimitReader(r.Body, 1<<16)).Decode(&req); err != nil || req.Seq < 1 {
		http.Error(w, `the body is {"seq": N}, the entry to revert`, http.StatusBadRequest)
		return
	}
	writer := writerOf(r, "anonymous")
	m, err := s.logModule()
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return
	}
	f.mu.Lock()
	defer f.mu.Unlock()
	for attempt := 0; attempt < 5; attempt++ {
		if err := s.refreshLogLocked(f); err != nil {
			logWriteFailed(w, err, http.StatusInternalServerError)
			return
		}
		if len(f.gaps) > 0 {
			logWriteFailed(w, &store.GapError{Log: f.name, Gaps: f.gaps}, http.StatusServiceUnavailable)
			return
		}
		if req.Seq > f.seq {
			http.Error(w, fmt.Sprintf("the %s log has no entry %d (its head is %d)", f.name, req.Seq, f.seq), http.StatusNotFound)
			return
		}
		cps, err := s.store.Checkpoints(f.name)
		if err != nil {
			fail(w, err, http.StatusInternalServerError)
			return
		}
		var base json.RawMessage
		var baseSeq int64
		for _, c := range cps {
			if c <= req.Seq-1 {
				baseSeq = c
			}
		}
		if baseSeq > 0 {
			if base, err = s.store.Checkpoint(f.name, baseSeq); err != nil {
				fail(w, err, http.StatusInternalServerError)
				return
			}
		}
		entries, err := s.store.Entries(f.name, baseSeq, f.seq)
		if err != nil {
			fail(w, err, http.StatusInternalServerError)
			return
		}
		nowMs := time.Now().UnixMilli()
		header := map[string]any{"log": f.name, "seq": f.seq + 1, "writer": writer, "at_ms": nowMs, "module": "cap-log@" + m.Hash(),
			"intent": map[string]any{"op": "POST /logs/" + f.name + "/revert"}}
		op := map[string]any{"op": "revert", "log": f.name, "entries": entries, "target": req.Seq, "header": header}
		if base != nil {
			op["checkpoint"] = base
		}
		var ans struct {
			Entry   string          `json:"entry"`
			Changes int             `json:"changes"`
			States  json.RawMessage `json:"states"`
		}
		if err := s.logOp(op, &ans); err != nil {
			http.Error(w, "the revert is refused: "+err.Error(), http.StatusConflict)
			return
		}
		if f.revise != nil {
			written, err := f.revise(ans.States, nowMs)
			if err != nil {
				http.Error(w, "the revert is refused: "+err.Error(), http.StatusConflict)
				return
			}
			op["written"] = written
			if err := s.logOp(op, &ans); err != nil {
				http.Error(w, "the revert is refused: "+err.Error(), http.StatusConflict)
				return
			}
		}
		err = s.store.AppendEntry(f.name, f.seq+1, []byte(ans.Entry))
		var ce *store.EntryConflictError
		if errors.As(err, &ce) {
			continue
		}
		if err != nil {
			fail(w, err, http.StatusInternalServerError)
			return
		}
		seq := f.seq + 1
		if err := s.refreshLogLocked(f); err != nil {
			log.Printf("%s log: after the revert: %v", f.name, err)
		}
		writeJSON(w, http.StatusOK, map[string]any{"log": f.name, "seq": seq, "reverts": req.Seq, "changes": ans.Changes})
		return
	}
	http.Error(w, fmt.Sprintf("the %s log moved on every attempt", f.name), http.StatusConflict)
}

// handleLogCloseGapOf writes the entry that closes a gap in a cap-log
// log: no changes, and the cause a person gives (D-190, rule 4).
func (s *server) handleLogCloseGapOf(w http.ResponseWriter, r *http.Request, f *logFold) {
	var req struct {
		Seq   int64  `json:"seq"`
		Cause string `json:"cause"`
	}
	if err := json.NewDecoder(io.LimitReader(r.Body, 1<<16)).Decode(&req); err != nil || req.Seq < 1 || req.Cause == "" {
		http.Error(w, `the body is {"seq": N, "cause": "why entry N is missing"}`, http.StatusBadRequest)
		return
	}
	m, err := s.logModule()
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return
	}
	header := map[string]any{"log": f.name, "seq": req.Seq, "writer": writerOf(r, "anonymous"), "at_ms": time.Now().UnixMilli(), "module": "cap-log@" + m.Hash(),
		"intent": map[string]any{"op": "POST /logs/" + f.name + "/close-gap", "note": req.Cause}}
	var ans struct {
		Entry string `json:"entry"`
	}
	if err := s.logOp(map[string]any{"op": "close_gap", "header": header}, &ans); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	if err := s.store.CloseGap(f.name, req.Seq, []byte(ans.Entry)); err != nil {
		fail(w, err, http.StatusConflict)
		return
	}
	log.Printf("%s log: entry %d closes the gap: %s", f.name, req.Seq, req.Cause)
	writeJSON(w, http.StatusOK, map[string]any{"log": f.name, "seq": req.Seq})
}
