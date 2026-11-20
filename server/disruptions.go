package main

// The disruptions log (D-190, D-197; docs/FORMATS.md, "The logs"). In
// store mode the record of the edit document (detours and trip cancels)
// is the disruptions log: one entry for each write, and the document is
// the fold of the entries. cap-log keeps the log (caplog.go); cap-sync
// answers its records (`log_records`), the document assembled from them
// (`log_document`), and what a revert writes back (`log_revert`: each
// record stamped to win the merge, and a record the reverted write
// created retired, D-193). The host decides nothing.
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
	"log"
	"net/http"
	"os"
	"time"

	"rustwasm/server/store"
)

// disruptionsLog is the disruptions log's name in the bucket.
const disruptionsLog = "disruptions"

// disruptionsFold is the server's fold of the disruptions log, made on
// first use.
func (s *server) disruptionsFold() *logFold {
	s.disruptionsOnce.Do(func() {
		s.disruptions = &logFold{
			name:      disruptionsLog,
			assemble:  s.assembleDisruptions,
			revise:    s.reviseDisruptions,
			documents: func() map[string]string { return map[string]string{"document": s.disruptionsDoc.hash} },
		}
	})
	return s.disruptions
}

// assembleDisruptions turns the fold's records into the edit document,
// through cap-sync (`log_document`). The empty log assembles the empty
// document. Caller holds the fold's lock.
func (s *server) assembleDisruptions(records map[string]json.RawMessage) error {
	req := map[string]any{"op": "log_document", "records": json.RawMessage("{}")}
	if raw, ok := records["edit_record"]; ok {
		req["records"] = raw
	}
	if own := ownRecord(records, "edit_records"); own != nil {
		req["document"] = own
	}
	var out struct {
		Document     string `json:"document"`
		DocumentHash string `json:"document_hash"`
	}
	if err := s.syncOp(req, &out); err != nil {
		return err
	}
	s.disruptionsDoc = logDoc{out.DocumentHash, []byte(out.Document)}
	return nil
}

// reviseDisruptions answers the records a revert writes back in place of
// the states before (cap-sync `log_revert`).
func (s *server) reviseDisruptions(states json.RawMessage, nowMs int64) (json.RawMessage, error) {
	var out struct {
		Written json.RawMessage `json:"written"`
	}
	err := s.syncOp(map[string]any{"op": "log_revert", "states": states, "now_ms": nowMs}, &out)
	return out.Written, err
}

// editRecords answers an edit document's records (cap-sync
// log_records): its edit records, and the document's own record.
func (s *server) editRecords(document string) ([]json.RawMessage, error) {
	var out logNext
	err := s.syncOp(map[string]any{"op": "log_records", "document": json.RawMessage(document)}, &out)
	return out.Next, err
}

// disruptionsNow answers the fold at the log's head. While the bucket
// does not answer, it answers the fold this process holds, stale
// (D-119).
func (s *server) disruptionsNow() (document []byte, head store.Head, err error) {
	f := s.disruptionsFold()
	stale, err := s.refreshLog(f)
	if err != nil {
		return nil, store.Head{Name: disruptionsLog}, err
	}
	f.mu.Lock()
	defer f.mu.Unlock()
	head = store.Head{Name: disruptionsLog, Hash: s.disruptionsDoc.hash, Seq: f.seq, Envelope: s.syncEnvelope(), AsOfMs: f.asOfMs, Stale: stale}
	return s.disruptionsDoc.doc, head, nil
}

// commitDisruptions runs one write through the receive path into one
// entry: merge the client's document with the fold, and write the
// records that changed. A lost race reads the log again and merges
// again (D-190, rule 2). from, when not nil, is the hash the writer last
// saw: a fold that moved answers *store.ConflictError, and nothing is
// written.
func (s *server) commitDisruptions(clientRaw json.RawMessage, writer string, from *string) (document, hash string, seq int64, unknown []string, err error) {
	f := s.disruptionsFold()
	nowMs := time.Now().UnixMilli()
	seq, _, _, err = s.writeLog(f, logWrite{writer: writer, op: "POST /edits", argsHash: store.HashOf(clientRaw)}, func() ([]json.RawMessage, *logRefusal, error) {
		held := s.disruptionsDoc
		if from != nil && *from != held.hash && !(*from == "" && f.seq == 0) {
			return nil, nil, &store.ConflictError{Pointer: disruptionsLog, Current: held.hash, Seq: f.seq}
		}
		var err error
		document, hash, unknown, err = s.mergeEdits(clientRaw, held.doc, nowMs)
		if err != nil {
			return nil, nil, err
		}
		next, err := s.editRecords(document)
		return next, nil, err
	})
	if err != nil {
		return "", "", 0, nil, err
	}
	return document, hash, seq, unknown, nil
}

// seedDisruptions gives an empty disruptions log its first entry at
// startup: the old `edits` pointer's head (writer migrate:edits, naming
// that head's hash), else the -edits file. A document with no records
// writes nothing. The pointer is never written again.
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
	records, document, _, dropped, err := s.readEdits(raw)
	if err != nil {
		log.Printf("seed the disruptions log: read_edits: %v", err)
		return
	}
	var held []json.RawMessage
	if json.Unmarshal(records, &held) == nil && len(held) == 0 {
		return
	}
	next, err := s.editRecords(document)
	if err != nil {
		log.Printf("seed the disruptions log: log_records: %v", err)
		return
	}
	seq, _, _, err := s.writeLog(s.disruptionsFold(), logWrite{writer: writer, op: "migrate", argsHash: argsHash}, func() ([]json.RawMessage, *logRefusal, error) {
		return next, nil, nil
	})
	if err != nil {
		log.Printf("seed the disruptions log: %v", err)
		return
	}
	log.Printf("seed the disruptions log: entry %d from %s (%s), %d malformed record(s) dropped", seq, writer, argsHash, dropped)
}

// handleLogs serves each log's entries and its revert and close-gap
// writes, over cap-log (caplog.go), with POST /logs/<log> the intent
// write of the stops, alerts, and notes logs (logwrite.go). Every read, revert, and closed gap needs
// the audit grant when sign-in is on (protectLog): the entries name
// their writers.
func (s *server) handleLogs(w http.ResponseWriter, r *http.Request) {
	if s.store == nil {
		http.Error(w, "no store configured (start with -store <url>)", http.StatusNotFound)
		return
	}
	var f *logFold
	switch r.URL.Path {
	case "/logs/disruptions", "/logs/disruptions/revert", "/logs/disruptions/close-gap":
		f = s.disruptionsFold()
	case "/logs/stops", "/logs/stops/revert", "/logs/stops/close-gap":
		f = s.stopsLog()
	case "/logs/alerts", "/logs/alerts/revert", "/logs/alerts/close-gap":
		f = s.alertsLog()
	case "/logs/notes", "/logs/notes/revert", "/logs/notes/close-gap":
		f = s.notesLog()
	case "/logs/plans", "/logs/plans/close-gap":
		f = s.plansLog()
	case "/logs/plans/revert":
		// A pick is corrected by the plans' own rules (D-75, D-76):
		// republish it, or retire it before it takes effect.
		http.Error(w, "the plans log has no revert: republish a pick to correct it, or retire it (POST /plans, /plans/retire)", http.StatusMethodNotAllowed)
		return
	default:
		http.Error(w, "no such log (the logs: disruptions, stops, plans, alerts, notes)", http.StatusNotFound)
		return
	}
	switch r.URL.Path {
	case "/logs/" + f.name:
		switch {
		case r.Method == http.MethodGet:
			s.handleLogEntriesOf(w, r, f)
		case r.Method == http.MethodPost && s.intentLogs()[f.name].fold != nil:
			s.handleIntentWrite(w, r, s.intentLogs()[f.name])
		case s.intentLogs()[f.name].fold != nil:
			http.Error(w, "use GET or POST", http.StatusMethodNotAllowed)
		default:
			http.Error(w, "use GET", http.StatusMethodNotAllowed)
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
