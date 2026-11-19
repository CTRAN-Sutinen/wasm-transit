package main

// The disruptions log (D-190; docs/FORMATS.md, "The logs"). In store
// mode the record of the edit document (detours and trip cancels) is the
// disruptions log: one entry for each write, and the document is the
// fold of the entries. The sync module
// (cap-sync) answers each entry, folds them, and answers a revert. This
// host only orders and keeps the entries (store/log.go), and treats
// every entry, checkpoint, and document as opaque bytes.
//
//	POST /edits                       a write: merge, then one entry (none when nothing changed)
//	GET  /edits                       the fold
//	GET  /logs/disruptions?since=N    the entries after N (audit grant)
//	POST /logs/disruptions/revert     {"seq": N}: the entry that reverts entry N (audit grant)
//	POST /logs/disruptions/close-gap  {"seq": N, "cause": "..."}: the entry that closes a gap (audit grant)
//
// The old `edits` pointer stays in the bucket, read only. A store that
// has it and no log gets the log's first entry from its head at startup
// (seedDisruptions).

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"sort"
	"strconv"
	"sync"
	"time"

	"rustwasm/server/store"
)

// disruptionsLog is the disruptions log's name in the bucket.
const disruptionsLog = "disruptions"

// checkpointEvery is how many entries the server folds past its newest
// checkpoint before it stores the next one. A boot reads the newest
// checkpoint and folds at most this many entries after it.
const checkpointEvery = 256

// disruptionsFold is this process's fold of the disruptions log.
type disruptionsFold struct {
	mu     sync.Mutex
	loaded bool
	// base is the checkpoint at seq, as the module answered it: the
	// next fold starts there.
	base json.RawMessage
	seq  int64
	// document and hash are the edit document at seq.
	document []byte
	hash     string
	// stored is the seq of the newest checkpoint in the bucket.
	stored int64
	// asOfMs is when the bucket last confirmed the head.
	asOfMs int64
	// quarantined names the entries the fold did not apply.
	quarantined map[int64]string
	// gaps are the missing entries the bucket listed last.
	gaps []int64
}

// foldAnswer is fold_disruptions' answer.
type foldAnswer struct {
	Seq          int64  `json:"seq"`
	Document     string `json:"document"`
	DocumentHash string `json:"document_hash"`
	Checkpoint   string `json:"checkpoint"`
	Quarantined  []struct {
		Seq    int64  `json:"seq"`
		Reason string `json:"reason"`
	} `json:"quarantined"`
}

// foldEdits asks the sync module for the fold of entries after a
// checkpoint (nil: from the empty document).
func (s *server) foldDisruptions(checkpoint json.RawMessage, entries []store.StoredEntry) (foldAnswer, error) {
	req := map[string]any{"op": "fold_disruptions", "entries": entries}
	if len(checkpoint) > 0 {
		req["checkpoint"] = checkpoint
	}
	if entries == nil {
		req["entries"] = []store.StoredEntry{}
	}
	var ans foldAnswer
	err := s.syncOp(req, &ans)
	return ans, err
}

// entryHeader is the place and provenance of an entry, as the module
// reads it.
func (s *server) entryHeader(seq int64, writer string, atMs int64, op, argsHash, note string) map[string]any {
	intent := map[string]any{"op": op}
	if argsHash != "" {
		intent["args_hash"] = argsHash
	}
	if note != "" {
		intent["note"] = note
	}
	return map[string]any{"seq": seq, "writer": writer, "at_ms": atMs, "module": "cap-sync@" + s.wasm.Hash(), "intent": intent}
}

// entryEdits asks the sync module for the entry from head to next: ""
// when the write changes nothing.
func (s *server) entryDisruptions(head, next []byte, header map[string]any) (string, error) {
	var ans struct {
		Entry string `json:"entry"`
	}
	err := s.syncOp(map[string]any{"op": "entry_disruptions", "head": json.RawMessage(head), "next": json.RawMessage(next), "header": header}, &ans)
	return ans.Entry, err
}

// bootDisruptionsLocked picks the checkpoint a fresh process starts from: the
// newest one that equals the fold from the checkpoint before it (or
// from the empty document). A checkpoint that differs raises an alarm,
// and the boot tries the one before it. Caller holds f.mu.
func (s *server) bootDisruptionsLocked(f *disruptionsFold, upto int64) error {
	cps, err := s.store.Checkpoints(disruptionsLog)
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
			if prev, err = s.store.Checkpoint(disruptionsLog, prevSeq); err != nil {
				return err
			}
		}
		stored, err := s.store.Checkpoint(disruptionsLog, at)
		if err != nil {
			return err
		}
		entries, err := s.store.Entries(disruptionsLog, prevSeq, at)
		if err != nil {
			return err
		}
		ans, err := s.foldDisruptions(prev, entries)
		if err == nil && ans.Seq == at && ans.Checkpoint == string(stored) {
			f.base, f.seq, f.stored = json.RawMessage(stored), at, at
			return nil
		}
		log.Printf("ALARM disruptions log: checkpoint %d differs from the fold of the entries before it (%v): the boot starts from an older one", at, err)
	}
	return nil
}

// refreshDisruptionsLocked brings the fold to the log's head: the entries up
// to the first gap, folded from the last fold. It stores a checkpoint
// every checkpointEvery entries. Caller holds f.mu.
func (s *server) refreshDisruptionsLocked() error {
	f := &s.disruptions
	head, err := s.store.LogHeadOf(disruptionsLog)
	if err != nil {
		return err
	}
	upto := head.Seq
	if len(head.Gaps) > 0 {
		upto = head.Gaps[0] - 1
		if len(f.gaps) == 0 || f.gaps[0] != head.Gaps[0] {
			log.Printf("ALARM disruptions log: a gap at %v: writes stop until POST /logs/disruptions/close-gap names the cause", head.Gaps)
		}
	}
	f.gaps = head.Gaps
	if !f.loaded {
		if err := s.bootDisruptionsLocked(f, upto); err != nil {
			return err
		}
		f.quarantined = map[int64]string{}
	}
	if upto > f.seq || !f.loaded {
		entries, err := s.store.Entries(disruptionsLog, f.seq, upto)
		if err != nil {
			return err
		}
		ans, err := s.foldDisruptions(f.base, entries)
		if err != nil {
			return fmt.Errorf("fold the disruptions log: %w", err)
		}
		for _, q := range ans.Quarantined {
			if _, seen := f.quarantined[q.Seq]; !seen {
				log.Printf("ALARM disruptions log: entry %d is quarantined: %s", q.Seq, q.Reason)
			}
			f.quarantined[q.Seq] = q.Reason
		}
		f.base, f.seq = json.RawMessage(ans.Checkpoint), ans.Seq
		f.document, f.hash = []byte(ans.Document), ans.DocumentHash
		f.loaded = true
		if f.seq-f.stored >= checkpointEvery {
			if err := s.store.PutCheckpoint(disruptionsLog, f.seq, f.base); err != nil {
				log.Printf("ALARM disruptions log: checkpoint %d: %v", f.seq, err)
			} else {
				f.stored = f.seq
			}
		}
	}
	f.asOfMs = head.AsOfMs
	return nil
}

// disruptionsNow answers the fold at the log's head. While the bucket does not
// answer, it answers the fold this process holds, stale (D-119).
func (s *server) disruptionsNow() (document []byte, head store.Head, err error) {
	f := &s.disruptions
	f.mu.Lock()
	defer f.mu.Unlock()
	err = s.refreshDisruptionsLocked()
	if err != nil && !(errors.Is(err, store.ErrUnavailable) && f.loaded) {
		return nil, store.Head{Name: disruptionsLog}, err
	}
	head = store.Head{Name: disruptionsLog, Hash: f.hash, Seq: f.seq, Envelope: s.syncEnvelope(), AsOfMs: f.asOfMs, Stale: err != nil}
	return f.document, head, nil
}

// commitDisruptions runs one write through the receive path into one
// entry: merge the client's document with the fold, answer the entry,
// and create it at the next seq. A lost race reads the log again and
// runs the receive path again (D-190, rule 2). from, when not nil, is
// the hash the writer last saw: a fold that moved answers
// *store.ConflictError, and nothing is written.
func (s *server) commitDisruptions(clientRaw json.RawMessage, writer string, from *string) (document, hash string, seq int64, unknown []string, err error) {
	f := &s.disruptions
	f.mu.Lock()
	defer f.mu.Unlock()
	nowMs := time.Now().UnixMilli()
	argsHash := store.HashOf(clientRaw)
	for attempt := 0; attempt < 5; attempt++ {
		if err := s.refreshDisruptionsLocked(); err != nil {
			return "", "", 0, nil, err
		}
		if len(f.gaps) > 0 {
			return "", "", 0, nil, &store.GapError{Log: disruptionsLog, Gaps: f.gaps}
		}
		if from != nil && *from != f.hash && !(*from == "" && f.seq == 0) {
			return "", "", 0, nil, &store.ConflictError{Pointer: disruptionsLog, Current: f.hash, Seq: f.seq}
		}
		document, hash, unknown, err := s.mergeEdits(clientRaw, f.document, nowMs)
		if err != nil {
			return "", "", 0, nil, err
		}
		entry, err := s.entryDisruptions(f.document, []byte(document), s.entryHeader(f.seq+1, writer, nowMs, "POST /edits", argsHash, ""))
		if err != nil {
			return "", "", 0, nil, err
		}
		if entry == "" {
			return document, hash, f.seq, unknown, nil // nothing changed: no entry
		}
		err = s.store.AppendEntry(disruptionsLog, f.seq+1, []byte(entry))
		var ce *store.EntryConflictError
		if errors.As(err, &ce) {
			continue
		}
		if err != nil {
			return "", "", 0, nil, err
		}
		return document, hash, f.seq + 1, unknown, nil
	}
	return "", "", 0, nil, fmt.Errorf("the disruptions log moved on every attempt")
}

// seedDisruptions gives an empty disruptions log its first entry at startup: the
// old `edits` pointer's head (writer migrate:edits, naming that head's
// hash), else the -edits file. An empty document writes nothing. The
// pointer is never written again.
func (s *server) seedDisruptions(editsPath string) {
	head, err := s.store.LogHeadOf(disruptionsLog)
	if err != nil || head.Seq > 0 || len(head.Gaps) > 0 {
		if err != nil {
			log.Printf("seed the disruptions log: %v", err)
		}
		return
	}
	var raw []byte
	writer, argsHash := "", ""
	if ptr, found, err := s.store.Head("edits"); err == nil && found {
		if raw, _, err = s.store.GetBlob(ptr.Hash); err != nil {
			log.Printf("seed the disruptions log: the edits pointer's head %s: %v", ptr.Hash, err)
			return
		}
		writer, argsHash = "migrate:edits", ptr.Hash
	} else if editsPath != "" {
		if raw, err = os.ReadFile(editsPath); err != nil || len(raw) == 0 {
			return
		}
		writer, argsHash = "migrate:-edits", store.HashOf(raw)
	} else {
		return
	}
	_, document, _, dropped, err := s.readEdits(raw)
	if err != nil {
		log.Printf("seed the disruptions log: read_edits: %v", err)
		return
	}
	entry, err := s.entryDisruptions([]byte(emptyEdits), []byte(document), s.entryHeader(1, writer, time.Now().UnixMilli(), "migrate", argsHash, ""))
	if err != nil || entry == "" {
		if err != nil {
			log.Printf("seed the disruptions log: entry_disruptions: %v", err)
		}
		return
	}
	var ce *store.EntryConflictError
	if err := s.store.AppendEntry(disruptionsLog, 1, []byte(entry)); err != nil && !errors.As(err, &ce) {
		log.Printf("seed the disruptions log: %v", err)
		return
	}
	log.Printf("seed the disruptions log: entry 1 from %s (%s), %d malformed record(s) dropped", writer, argsHash, dropped)
}

// handleLogs serves each log's entries and its revert and close-gap
// writes: /logs/disruptions here, /logs/stops over cap-log (caplog.go),
// with POST /logs/stops the stops log's writes (stopslog.go). Every
// read, revert, and closed gap needs the audit grant when sign-in is on
// (protectLog): the entries name their writers.
func (s *server) handleLogs(w http.ResponseWriter, r *http.Request) {
	if s.store == nil {
		http.Error(w, "no store configured (start with -store <url>)", http.StatusNotFound)
		return
	}
	switch r.URL.Path {
	case "/logs/disruptions":
		if r.Method != http.MethodGet {
			http.Error(w, "use GET", http.StatusMethodNotAllowed)
			return
		}
		s.handleLogEntries(w, r)
	case "/logs/disruptions/revert":
		if r.Method != http.MethodPost {
			http.Error(w, "use POST", http.StatusMethodNotAllowed)
			return
		}
		s.handleLogRevert(w, r)
	case "/logs/disruptions/close-gap":
		if r.Method != http.MethodPost {
			http.Error(w, "use POST", http.StatusMethodNotAllowed)
			return
		}
		s.handleLogCloseGap(w, r)
	case "/logs/stops":
		switch r.Method {
		case http.MethodGet:
			s.handleLogEntriesOf(w, r, s.stopsLog())
		case http.MethodPost:
			s.handleStopsOps(w, r)
		default:
			http.Error(w, "use GET or POST", http.StatusMethodNotAllowed)
		}
	case "/logs/stops/revert", "/logs/stops/close-gap":
		if r.Method != http.MethodPost {
			http.Error(w, "use POST", http.StatusMethodNotAllowed)
			return
		}
		if r.URL.Path == "/logs/stops/revert" {
			s.handleLogRevertOf(w, r, s.stopsLog())
		} else {
			s.handleLogCloseGapOf(w, r, s.stopsLog())
		}
	default:
		http.Error(w, "no such log (the logs: disruptions, stops)", http.StatusNotFound)
	}
}

// logEntriesLimit is the most entries one GET /logs/disruptions answers.
const logEntriesLimit = 1000

// handleLogEntries answers the entries after `since` (0: from the
// first), at most `limit`, with the log's head, gaps, newest
// checkpoint, and the entries the fold quarantined.
func (s *server) handleLogEntries(w http.ResponseWriter, r *http.Request) {
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
	_, head, err := s.disruptionsNow()
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return
	}
	lh, err := s.store.LogHeadOf(disruptionsLog)
	if err != nil {
		lh = store.LogHead{Log: disruptionsLog, Seq: head.Seq}
	}
	stored, err := s.store.Entries(disruptionsLog, since, min(lh.Seq, since+limit))
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
	s.disruptions.mu.Lock()
	quarantined := make([]held, 0, len(s.disruptions.quarantined))
	for seq, why := range s.disruptions.quarantined {
		quarantined = append(quarantined, held{seq, why})
	}
	s.disruptions.mu.Unlock()
	sort.Slice(quarantined, func(i, j int) bool { return quarantined[i].Seq < quarantined[j].Seq })
	labelStale(w, head)
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Cache-Control", "no-cache")
	json.NewEncoder(w).Encode(map[string]any{
		"log":         disruptionsLog,
		"head":        lh.Seq,
		"gaps":        lh.Gaps,
		"checkpoint":  lh.Checkpoint,
		"document":    head.Hash,
		"entries":     items,
		"quarantined": quarantined,
	})
}

// handleLogRevert writes the entry that reverts entry `seq`. The module
// refuses it, naming the record, when a later entry changed one of the
// records: that answers 409, because a person decides.
func (s *server) handleLogRevert(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Seq int64 `json:"seq"`
	}
	if err := json.NewDecoder(io.LimitReader(r.Body, 1<<16)).Decode(&req); err != nil || req.Seq < 1 {
		http.Error(w, `the body is {"seq": N}, the entry to revert`, http.StatusBadRequest)
		return
	}
	writer := writerOf(r, "anonymous")
	f := &s.disruptions
	f.mu.Lock()
	defer f.mu.Unlock()
	for attempt := 0; attempt < 5; attempt++ {
		if err := s.refreshDisruptionsLocked(); err != nil {
			fail(w, err, http.StatusInternalServerError)
			return
		}
		if len(f.gaps) > 0 {
			http.Error(w, (&store.GapError{Log: disruptionsLog, Gaps: f.gaps}).Error(), http.StatusServiceUnavailable)
			return
		}
		if req.Seq > f.seq {
			http.Error(w, fmt.Sprintf("the disruptions log has no entry %d (its head is %d)", req.Seq, f.seq), http.StatusNotFound)
			return
		}
		// The newest checkpoint before the entry, else the empty document.
		cps, err := s.store.Checkpoints(disruptionsLog)
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
			if base, err = s.store.Checkpoint(disruptionsLog, baseSeq); err != nil {
				fail(w, err, http.StatusInternalServerError)
				return
			}
		}
		entries, err := s.store.Entries(disruptionsLog, baseSeq, f.seq)
		if err != nil {
			fail(w, err, http.StatusInternalServerError)
			return
		}
		op := map[string]any{"op": "revert_disruptions", "entries": entries, "target": req.Seq,
			"header": s.entryHeader(f.seq+1, writer, time.Now().UnixMilli(), "POST /logs/disruptions/revert", "", "")}
		if base != nil {
			op["checkpoint"] = base
		}
		var ans struct {
			Entry   string `json:"entry"`
			Changes int    `json:"changes"`
		}
		if err := s.syncOp(op, &ans); err != nil {
			http.Error(w, "the revert is refused: "+err.Error(), http.StatusConflict)
			return
		}
		err = s.store.AppendEntry(disruptionsLog, f.seq+1, []byte(ans.Entry))
		var ce *store.EntryConflictError
		if errors.As(err, &ce) {
			continue
		}
		if err != nil {
			fail(w, err, http.StatusInternalServerError)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{"log": disruptionsLog, "seq": f.seq + 1, "reverts": req.Seq, "changes": ans.Changes})
		return
	}
	http.Error(w, "the disruptions log moved on every attempt", http.StatusConflict)
}

// handleLogCloseGap writes the entry that closes a gap: no changes, and
// the cause a person gives (D-190, rule 4).
func (s *server) handleLogCloseGap(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Seq   int64  `json:"seq"`
		Cause string `json:"cause"`
	}
	if err := json.NewDecoder(io.LimitReader(r.Body, 1<<16)).Decode(&req); err != nil || req.Seq < 1 || req.Cause == "" {
		http.Error(w, `the body is {"seq": N, "cause": "why entry N is missing"}`, http.StatusBadRequest)
		return
	}
	var ans struct {
		Entry string `json:"entry"`
	}
	header := s.entryHeader(req.Seq, writerOf(r, "anonymous"), time.Now().UnixMilli(), "POST /logs/disruptions/close-gap", "", req.Cause)
	if err := s.syncOp(map[string]any{"op": "close_disruptions_gap", "header": header}, &ans); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	if err := s.store.CloseGap(disruptionsLog, req.Seq, []byte(ans.Entry)); err != nil {
		fail(w, err, http.StatusConflict)
		return
	}
	log.Printf("disruptions log: entry %d closes the gap: %s", req.Seq, req.Cause)
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]any{"log": disruptionsLog, "seq": req.Seq})
}
