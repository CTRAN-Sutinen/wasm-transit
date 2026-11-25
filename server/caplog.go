package main

// The host half of the logs that cap-log keeps (D-190, D-197;
// docs/FORMATS.md, "The logs"). The host orders and stores entries and
// checkpoints, and treats every entry, checkpoint, record, and document
// as opaque bytes. cap-log folds them and never reads a record.

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

const defaultLogWasm = "../web/pkg/cap_log.wasm"

// A boot folds at most this many entries after the newest checkpoint.
const checkpointEvery = 256

const logEntriesLimit = 1000

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

type logFold struct {
	name     string
	assemble func(records map[string]json.RawMessage) error
	// revise is set for a kind whose merge would undo a revert (D-193).
	// It answers the records that the revert writes back (cap-log
	// `written`) in place of the states before.
	revise func(states json.RawMessage, nowMs int64) (json.RawMessage, error)
	// documents names the hashes of the documents the fold serves.
	// The caller holds mu.
	documents func() map[string]string

	mu     sync.Mutex
	loaded bool
	base   json.RawMessage
	seq    int64
	// stored is the seq of the newest checkpoint in the bucket.
	stored int64
	asOfMs int64
	// stale means the bucket did not answer, so the head is the last one
	// that this process or its disk cache confirmed (D-119).
	stale       bool
	quarantined map[int64]string
	gaps        []int64
}

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

// bootLogLocked starts from the newest checkpoint that equals the fold
// from the checkpoint before it, or from the empty log. A checkpoint
// that differs raises an alarm. The caller holds f.mu.
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

// refreshLogLocked folds only the entries up to the first gap. The
// caller holds f.mu.
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
		if !head.Stale && f.seq-f.stored >= checkpointEvery {
			if err := s.store.PutCheckpoint(f.name, f.seq, f.base); err != nil {
				log.Printf("ALARM %s log: checkpoint %d: %v", f.name, f.seq, err)
			} else {
				f.stored = f.seq
			}
		}
	}
	f.asOfMs, f.stale = head.AsOfMs, head.Stale
	return nil
}

func errStaleHead(f *logFold) error {
	return fmt.Errorf("%w: the %s log's head is as of %d", store.ErrUnavailable, f.name, f.asOfMs)
}

// refreshLog keeps a loaded fold, and reports it stale, while the bucket
// does not answer (D-119).
func (s *server) refreshLog(f *logFold) (stale bool, err error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	err = s.refreshLogLocked(f)
	if err != nil && errors.Is(err, store.ErrUnavailable) && f.loaded {
		return true, nil
	}
	return err == nil && f.stale, err
}

type logRefusal struct {
	status int
	body   any
}

// readSeq, when set, is the log's seq that the writer read. A write is
// refused (409) when it changes a record that a later entry changed.
// claimedAtMs is the device's clock when the person made the write, 0 if
// the writer gives none. It is a claim beside the server's stamp (D-222).
type logWrite struct {
	writer, op, argsHash, key, note string
	readSeq                         *int64
	// readSeqs holds the seqs of the other logs that the writer read
	// (the manifest's log_seqs, D-233). A log not named was read now.
	readSeqs    map[string]int64
	claimedAtMs int64
}

type logOverlap struct {
	Kind string `json:"kind"`
	ID   string `json:"id"`
	Seq  int64  `json:"seq"`
}

// keyLanded composes with nothing to write, and cap-log answers the
// entry that holds the idempotency key. The caller holds f.mu.
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

// logWriteFailed answers 503 for an unavailable bucket or a gap, because
// nothing was written and the client can resend. Other errors use code.
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

// handleLogRevertOf answers 409 when cap-log refuses the revert because
// a later entry changed one of the records: a person decides. When
// f.revise is set, it answers the records over cap-log's states, and
// cap-log builds the entry again with them.
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
		if err := s.refreshLogLocked(f); err == nil && f.stale {
			logWriteFailed(w, errStaleHead(f), http.StatusServiceUnavailable)
			return
		} else if err != nil {
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
		answer := map[string]any{"log": f.name, "seq": seq, "reverts": req.Seq, "changes": ans.Changes}
		// Release f.mu while reading every log's fold (D-233). The
		// deferred unlock needs it held again.
		f.mu.Unlock()
		refs := s.revertReferences(f.name, seq, ans.Entry)
		f.mu.Lock()
		if len(refs) > 0 {
			answer["references"] = refs
		}
		writeJSON(w, http.StatusOK, answer)
		return
	}
	http.Error(w, fmt.Sprintf("the %s log moved on every attempt", f.name), http.StatusConflict)
}

// handleLogCloseGapOf writes an entry that names the cause (D-190, rule 4).
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

// revertReferences answers the references that the revert's entry broke
// by taking out their target, judged over every log's fold (D-233). A
// check that fails is logged and answers none.
func (s *server) revertReferences(logName string, seq int64, entry string) []json.RawMessage {
	t, err := s.kindTable()
	if err != nil || t.raw == nil || entry == "" {
		return nil
	}
	folds := map[string]json.RawMessage{}
	for name, kept := range s.keptLogs() {
		f := kept.fold
		f.mu.Lock()
		err := s.refreshLogLocked(f)
		base := f.base
		f.mu.Unlock()
		if err != nil {
			log.Printf("references: the %s log: %v", name, err)
			continue
		}
		var c struct {
			Records json.RawMessage `json:"records"`
		}
		if json.Unmarshal(base, &c) == nil && c.Records != nil {
			folds[f.name] = c.Records
		}
	}
	var got struct {
		Broken []json.RawMessage `json:"broken"`
	}
	req := map[string]any{"op": "references", "kinds": t.raw, "folds": folds, "log": logName, "entry": map[string]any{"seq": seq, "text": entry}}
	if err := s.logOp(req, &got); err != nil {
		log.Printf("references: the %s log's entry %d: %v", logName, seq, err)
		return nil
	}
	for _, b := range got.Broken {
		log.Printf("references: the %s log's entry %d leaves a broken reference: %s", logName, seq, b)
	}
	return got.Broken
}

// foldAt reads the store, never another log's fold, so a write that
// holds its own log's lock can call it.
func (s *server) foldAt(name string, seq int64) (json.RawMessage, error) {
	if seq <= 0 {
		return json.RawMessage("{}"), nil
	}
	cps, err := s.store.Checkpoints(name)
	if err != nil {
		return nil, err
	}
	var from int64
	for _, c := range cps {
		if c <= seq && c > from {
			from = c
		}
	}
	req := map[string]any{"op": "fold", "log": name}
	if from > 0 {
		cp, err := s.store.Checkpoint(name, from)
		if err != nil {
			return nil, err
		}
		req["checkpoint"] = json.RawMessage(cp)
	}
	entries, err := s.store.Entries(name, from, seq)
	if err != nil {
		return nil, err
	}
	req["entries"] = entries
	var got struct {
		Records json.RawMessage `json:"records"`
	}
	if err := s.logOp(req, &got); err != nil {
		return nil, err
	}
	return got.Records, nil
}

// referenceFolds answers what a write's receive op judges its references
// over (D-233). read holds each log's records as of the seq that the
// writer read, and only when judged (a person's write). A log with no
// entries is left out. The caller holds the lock of f, the write's own
// log, so the other logs come from the store and no other lock is taken.
// kinds is nil when no table exists or a log cannot be read, and then
// nothing is refused.
func (s *server) referenceFolds(f *logFold, w logWrite, judged bool) (kinds json.RawMessage, read, now map[string]json.RawMessage) {
	t, err := s.kindTable()
	if err != nil || t.raw == nil {
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
	for name := range s.keptLogs() {
		head := f.seq
		if name != f.name {
			h, err := s.store.LogHeadOf(name)
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
		} else if headRecords, err = s.foldAt(name, head); err != nil {
			log.Printf("references: the %s log: %v", name, err)
			return nil, nil, nil
		}
		now[name] = headRecords
		if !judged {
			continue
		}
		at, ok := w.readSeqs[name]
		if name == f.name {
			ok = w.readSeq != nil
			if ok {
				at = *w.readSeq
			}
		}
		switch {
		case !ok || at >= head:
			read[name] = headRecords
		case at > 0:
			if read[name], err = s.foldAt(name, at); err != nil {
				log.Printf("references: the %s log as of %d: %v", name, at, err)
				return nil, nil, nil
			}
		}
	}
	return t.raw, read, now
}
