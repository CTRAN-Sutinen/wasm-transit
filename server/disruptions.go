package main

// The disruptions log (D-190; docs/FORMATS.md, "The logs"). In
// store mode the edit document is the fold of the log's entries. The
// receive op writes it (logkinds.go; cap-sync `receive`), and the host
// decides nothing.
//
// The old `edits` pointer stays in the bucket, read only.

import (
	"net/http"
	"strings"

	"rustwasm/server/store"
)

const disruptionsLog = "disruptions"

// While the bucket does not answer, it answers the fold this process
// holds, stale.
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

// Every read, revert, and closed gap needs the audit grant when sign-in
// is on (protectLog), because the entries name their writers.
func (s *server) handleLogs(w http.ResponseWriter, r *http.Request) {
	if s.store == nil {
		http.Error(w, "no store configured (start with -store <url>)", http.StatusNotFound)
		return
	}
	var f *logFold
	switch r.URL.Path {
	case "/logs/plans/revert":
		// A pick is corrected by republishing or retiring it.
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
			// Only the kind's routes write it (the kind table's `intent`).
			// A freeze stores its network first, and POST /edits is the edit
			// document's protocol.
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
