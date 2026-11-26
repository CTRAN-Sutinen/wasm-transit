package logs

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net/http"
	"sort"
	"strconv"
	"strings"
	"time"

	"rustwasm/server/httpx"
	"rustwasm/server/kinds"
	"rustwasm/server/store"
)

// /logs/: the entries, a revert, a closed gap, and an intent write.

// A Post is a kind's `post` route and the log that receives it.
type Post struct {
	Kind kinds.Entry
	Log  *Log
}

// Posts are the kinds' `post` routes, by path.
func (ls *Logs) Posts() map[string]Post {
	out := map[string]Post{}
	for _, l := range ls.all() {
		for _, k := range l.kinds {
			if k.Post != nil {
				out[k.Post.Path] = Post{k, l}
			}
		}
	}
	return out
}

// IsLog says whether /logs/<name> is a log the server keeps.
func (ls *Logs) IsLog(name string) bool { return ls.Get(name) != nil }

const logEntriesLimit = 1000

func (ls *Logs) handleLogEntriesOf(w http.ResponseWriter, r *http.Request, f *fold) {
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
	stale, err := ls.refreshLog(f)
	if err != nil {
		httpx.Fail(w, err, http.StatusInternalServerError)
		return
	}
	lh, err := ls.store.LogHeadOf(f.name)
	if err != nil {
		lh = store.LogHead{Log: f.name, Seq: f.seq}
	}
	stored, err := ls.store.Entries(f.name, since, min(lh.Seq, since+limit))
	if err != nil {
		httpx.Fail(w, err, http.StatusInternalServerError)
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
	httpx.LabelStale(w, store.Head{Stale: stale, AsOfMs: asOf})
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Cache-Control", "no-cache")
	json.NewEncoder(w).Encode(answer)
}

// handleLogRevertOf answers 409 when cap-log refuses the revert because
// a later entry changed one of the records: a person decides. When
// f.revise is set, it answers the records over cap-log's states, and
// cap-log builds the entry again with them.
func (ls *Logs) handleLogRevertOf(w http.ResponseWriter, r *http.Request, f *fold) {
	var req struct {
		Seq int64 `json:"seq"`
	}
	if err := json.NewDecoder(io.LimitReader(r.Body, 1<<16)).Decode(&req); err != nil || req.Seq < 1 {
		http.Error(w, `the body is {"seq": N}, the entry to revert`, http.StatusBadRequest)
		return
	}
	writer := httpx.Writer(r, "anonymous")
	m, err := ls.logModule()
	if err != nil {
		httpx.Fail(w, err, http.StatusInternalServerError)
		return
	}
	f.mu.Lock()
	defer f.mu.Unlock()
	for attempt := 0; attempt < 5; attempt++ {
		if err := ls.refreshLogLocked(f); err == nil && f.stale {
			WriteFailed(w, errStaleHead(f), http.StatusServiceUnavailable)
			return
		} else if err != nil {
			WriteFailed(w, err, http.StatusInternalServerError)
			return
		}
		if len(f.gaps) > 0 {
			WriteFailed(w, &store.GapError{Log: f.name, Gaps: f.gaps}, http.StatusServiceUnavailable)
			return
		}
		if req.Seq > f.seq {
			http.Error(w, fmt.Sprintf("the %s log has no entry %d (its head is %d)", f.name, req.Seq, f.seq), http.StatusNotFound)
			return
		}
		cps, err := ls.store.Checkpoints(f.name)
		if err != nil {
			httpx.Fail(w, err, http.StatusInternalServerError)
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
			if base, err = ls.store.Checkpoint(f.name, baseSeq); err != nil {
				httpx.Fail(w, err, http.StatusInternalServerError)
				return
			}
		}
		entries, err := ls.store.Entries(f.name, baseSeq, f.seq)
		if err != nil {
			httpx.Fail(w, err, http.StatusInternalServerError)
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
		if err := ls.logOp(op, &ans); err != nil {
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
			if err := ls.logOp(op, &ans); err != nil {
				http.Error(w, "the revert is refused: "+err.Error(), http.StatusConflict)
				return
			}
		}
		err = ls.store.AppendEntry(f.name, f.seq+1, []byte(ans.Entry))
		var ce *store.EntryConflictError
		if errors.As(err, &ce) {
			continue
		}
		if err != nil {
			httpx.Fail(w, err, http.StatusInternalServerError)
			return
		}
		seq := f.seq + 1
		if err := ls.refreshLogLocked(f); err != nil {
			log.Printf("%s log: after the revert: %v", f.name, err)
		}
		answer := map[string]any{"log": f.name, "seq": seq, "reverts": req.Seq, "changes": ans.Changes}
		// Release f.mu while reading every log's fold (D-233). The
		// deferred unlock needs it held again.
		f.mu.Unlock()
		refs := ls.revertReferences(f.name, seq, ans.Entry)
		f.mu.Lock()
		if len(refs) > 0 {
			answer["references"] = refs
		}
		httpx.JSON(w, http.StatusOK, answer)
		return
	}
	http.Error(w, fmt.Sprintf("the %s log moved on every attempt", f.name), http.StatusConflict)
}

// handleLogCloseGapOf writes an entry that names the cause (D-190, rule 4).
func (ls *Logs) handleLogCloseGapOf(w http.ResponseWriter, r *http.Request, f *fold) {
	var req struct {
		Seq   int64  `json:"seq"`
		Cause string `json:"cause"`
	}
	if err := json.NewDecoder(io.LimitReader(r.Body, 1<<16)).Decode(&req); err != nil || req.Seq < 1 || req.Cause == "" {
		http.Error(w, `the body is {"seq": N, "cause": "why entry N is missing"}`, http.StatusBadRequest)
		return
	}
	m, err := ls.logModule()
	if err != nil {
		httpx.Fail(w, err, http.StatusInternalServerError)
		return
	}
	header := map[string]any{"log": f.name, "seq": req.Seq, "writer": httpx.Writer(r, "anonymous"), "at_ms": time.Now().UnixMilli(), "module": "cap-log@" + m.Hash(),
		"intent": map[string]any{"op": "POST /logs/" + f.name + "/close-gap", "note": req.Cause}}
	var ans struct {
		Entry string `json:"entry"`
	}
	if err := ls.logOp(map[string]any{"op": "close_gap", "header": header}, &ans); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	if err := ls.store.CloseGap(f.name, req.Seq, []byte(ans.Entry)); err != nil {
		httpx.Fail(w, err, http.StatusConflict)
		return
	}
	log.Printf("%s log: entry %d closes the gap: %s", f.name, req.Seq, req.Cause)
	httpx.JSON(w, http.StatusOK, map[string]any{"log": f.name, "seq": req.Seq})
}

// revertReferences answers the references that the revert's entry broke
// by taking out their target, judged over every log's fold (D-233). A
// check that fails is logged and answers none.
func (ls *Logs) revertReferences(logName string, seq int64, entry string) []json.RawMessage {
	t, err := ls.table()
	if err != nil || t.Raw == nil || entry == "" {
		return nil
	}
	folds := map[string]json.RawMessage{}
	for name, kept := range ls.all() {
		f := kept.fold
		f.mu.Lock()
		err := ls.refreshLogLocked(f)
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
	req := map[string]any{"op": "references", "kinds": t.Raw, "folds": folds, "log": logName, "entry": map[string]any{"seq": seq, "text": entry}}
	if err := ls.logOp(req, &got); err != nil {
		log.Printf("references: the %s log's entry %d: %v", logName, seq, err)
		return nil
	}
	for _, b := range got.Broken {
		log.Printf("references: the %s log's entry %d leaves a broken reference: %s", logName, seq, b)
	}
	return got.Broken
}

func (ls *Logs) keptRoutesOf(name string) string {
	var routes []string
	seen := map[string]bool{}
	if l := ls.Get(name); l != nil {
		for _, k := range l.kinds {
			if k.Route != "" && !seen[k.Route] {
				seen[k.Route] = true
				routes = append(routes, k.Route)
			}
		}
	}
	return strings.Join(routes, "; ")
}

func (ls *Logs) handleIntent(w http.ResponseWriter, r *http.Request, l *Log) {
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
	write := Write{Writer: httpx.Writer(r, "anonymous"), Op: "POST /logs/" + l.name, ArgsHash: store.HashOf(body), Key: req.Key, ReadSeq: req.ReadSeq, ReadSeqs: req.ReadSeqs, ClaimedAtMs: req.ClaimedAtMs}
	seq, landed, got, refusal, err := ls.Receive(r.Context(), r, l, write, Call{Ops: req.Ops})
	if err != nil {
		WriteFailed(w, err, http.StatusBadRequest)
		return
	}
	if refusal != nil {
		WriteRefused(w, refusal)
		return
	}
	httpx.FlagUnknown(w, "the "+l.name+" log", write.Writer, got.UnknownFields)
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
	httpx.JSON(w, http.StatusOK, answer)
}

// HandleLogs serves /logs/. Every read, revert, and closed gap needs the audit grant when sign-in
// is on (protectLog), because the entries name their writers.
func (ls *Logs) HandleLogs(w http.ResponseWriter, r *http.Request) {
	if ls.store == nil {
		http.Error(w, "no store configured (start with -store <url>)", http.StatusNotFound)
		return
	}
	var f *fold
	switch r.URL.Path {
	case "/logs/plans/revert":
		// A pick is corrected by republishing or retiring it.
		http.Error(w, "the plans log has no revert: republish a pick to correct it, or retire it (POST /plans, /plans/retire)", http.StatusMethodNotAllowed)
		return
	default:
		name, _, _ := strings.Cut(strings.TrimPrefix(r.URL.Path, "/logs/"), "/")
		kept := ls.Get(name)
		if kept == nil {
			http.Error(w, "no such log (the logs: "+strings.Join(ls.Names(), ", ")+")", http.StatusNotFound)
			return
		}
		f = kept.fold
	}
	switch r.URL.Path {
	case "/logs/" + f.name:
		switch {
		case r.Method == http.MethodGet:
			ls.handleLogEntriesOf(w, r, f)
		case !ls.Get(f.name).intent:
			// Only the kind's routes write it (the kind table's `intent`).
			// A freeze stores its network first, and POST /edits is the edit
			// document's protocol.
			http.Error(w, "use GET (the "+f.name+" log is written through its kinds' routes: "+ls.keptRoutesOf(f.name)+")", http.StatusMethodNotAllowed)
		case r.Method == http.MethodPost:
			ls.handleIntent(w, r, ls.Get(f.name))
		default:
			http.Error(w, "use GET or POST", http.StatusMethodNotAllowed)
		}
	case "/logs/" + f.name + "/revert":
		if r.Method != http.MethodPost {
			http.Error(w, "use POST", http.StatusMethodNotAllowed)
			return
		}
		ls.handleLogRevertOf(w, r, f)
	default:
		if r.Method != http.MethodPost {
			http.Error(w, "use POST", http.StatusMethodNotAllowed)
			return
		}
		ls.handleLogCloseGapOf(w, r, f)
	}
}
