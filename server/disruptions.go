package main

// The disruptions log (D-190, D-197; docs/FORMATS.md, "The logs"). In
// store mode the record of the edit document (detours and trip cancels)
// is the disruptions log: one entry for each write, and the document is
// the fold of the entries. Its receive op writes it (logkinds.go;
// cap-sync `receive`): the sync protocol's merge, the tombstone
// compaction, and the first entry from the old `edits` pointer or the
// dev store's -edits file. A revert writes back the records it puts
// back, stamped to win the merge (cap-sync `log_revert`, D-193). The
// host decides nothing.
//
//	POST /edits                       a write: merge, then one entry (none when nothing changed)
//	GET  /edits                       the fold
//	GET  /logs/disruptions?since=N    the entries after N (audit grant)
//	POST /logs/disruptions/revert     {"seq": N}: the entry that reverts entry N (audit grant)
//	POST /logs/disruptions/close-gap  {"seq": N, "cause": "..."}: the entry that closes a gap (audit grant)
//
// The old `edits` pointer stays in the bucket, read only.

import (
	"net/http"
	"strings"

	"rustwasm/server/store"
)

// disruptionsLog is the disruptions log's name in the bucket.
const disruptionsLog = "disruptions"

// disruptionsNow answers the fold at the log's head. While the bucket
// does not answer, it answers the fold this process holds, stale
// (D-119).
func (s *server) disruptionsNow() (document []byte, head store.Head, err error) {
	l := s.keptLog(disruptionsLog)
	if l == nil {
		return nil, store.Head{Name: disruptionsLog}, errNoLog(disruptionsLog)
	}
	stale, err := s.refreshLog(l.fold)
	if err != nil {
		return nil, store.Head{Name: disruptionsLog}, err
	}
	l.fold.mu.Lock()
	defer l.fold.mu.Unlock()
	held := l.docs["edit_records"]
	head = store.Head{Name: disruptionsLog, Hash: held.hash, Seq: l.fold.seq, Envelope: s.syncEnvelope(), AsOfMs: l.fold.asOfMs, Stale: stale}
	return held.doc, head, nil
}

// handleLogs serves each log's entries and its revert and close-gap
// writes, over cap-log (caplog.go), with POST /logs/<log> the intent
// write of each log a receive op writes (logkinds.go). Every read, revert, and closed gap needs the
// audit grant when sign-in is on (protectLog): the entries name their
// writers.
func (s *server) handleLogs(w http.ResponseWriter, r *http.Request) {
	if s.store == nil {
		http.Error(w, "no store configured (start with -store <url>)", http.StatusNotFound)
		return
	}
	var f *logFold
	switch r.URL.Path {
	case "/logs/plans/revert":
		// A pick is corrected by the plans' own rules (D-75, D-76):
		// republish it, or retire it before it takes effect.
		http.Error(w, "the plans log has no revert: republish a pick to correct it, or retire it (POST /plans, /plans/retire)", http.StatusMethodNotAllowed)
		return
	default:
		name, _, _ := strings.Cut(strings.TrimPrefix(r.URL.Path, "/logs/"), "/")
		kept := s.keptLog(name)
		if kept == nil {
			http.Error(w, "no such log (the logs: "+strings.Join(s.keptNames(), ", ")+")", http.StatusNotFound)
			return
		}
		f = kept.fold
	}
	switch r.URL.Path {
	case "/logs/" + f.name:
		switch {
		case r.Method == http.MethodGet:
			s.handleLogEntriesOf(w, r, f)
		case !s.keptLog(f.name).intent:
			// Only the kind's routes write it (the kind table's
			// `intent`): a freeze stores its network first, and POST
			// /edits is the edit document's protocol.
			http.Error(w, "use GET (the "+f.name+" log is written through its kinds' routes: "+s.keptRoutesOf(f.name)+")", http.StatusMethodNotAllowed)
		case r.Method == http.MethodPost:
			s.handleKeptIntent(w, r, s.keptLog(f.name))
		default:
			http.Error(w, "use GET or POST", http.StatusMethodNotAllowed)
		}
	case "/logs/" + f.name + "/revert":
		if r.Method != http.MethodPost {
			http.Error(w, "use POST", http.StatusMethodNotAllowed)
			return
		}
		s.handleLogRevertOf(w, r, f)
	default:
		if r.Method != http.MethodPost {
			http.Error(w, "use POST", http.StatusMethodNotAllowed)
			return
		}
		s.handleLogCloseGapOf(w, r, f)
	}
}
