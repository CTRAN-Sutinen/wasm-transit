package main

// The notes document (store mode; D-214): the agency's internal notes.
// A write is one note, not a whole document. The receive path runs
// cap-notes `write` over the head, with the verified writer and the
// server's clock, so a page never names who wrote a note. cap-notes
// answers every refusal (422) or the next document, which the notes log
// records as one entry. Two people who note at once do not conflict:
// each write lands on the head that the log holds when it lands.
//
// One cap-notes instance serves every use, emptied after each
// (`withNotes`).
//
//	POST /notes   body: {"id": "<the note's id>", "target": {...}, "text": "..."}
//	              200 {notes_hash, id}   422 {refusals}
//
// The manifest pins the head as notes_hash; a page reads the document by
// hash from /blobs. The log is logs/notes/ in the bucket (GET
// /logs/notes). No GTFS-rt feed carries a note.

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"

	"rustwasm/server/store"
)

const (
	notesLogName = "notes"
	notesPointer = "notes"
	opLoadNotes  = 274
)

// withNotes runs `use` on the server's one cap-notes instance, made on
// first use, and then empties it (the kernel's evict), so no document
// outlives the request that sent it.
func (s *server) withNotes(ctx context.Context, use func(*WasmHost) error) error {
	if s.notesWasm == "" {
		return fmt.Errorf("no notes module (start with -notes-wasm)")
	}
	s.notesMu.Lock()
	defer s.notesMu.Unlock()
	if s.notesHeld == nil {
		host, err := NewWasmHost(ctx, s.notesWasm)
		if err != nil {
			return fmt.Errorf("notes module: %w", err)
		}
		s.notesHeld = host
	}
	err := use(s.notesHeld)
	var evicted json.RawMessage
	if evictErr := s.notesHeld.InvokeBytes(ctx, opEvict, []byte(`{"keep":[]}`), &evicted); evictErr != nil && err == nil {
		err = fmt.Errorf("emptying the notes module: %w", evictErr)
	}
	return err
}

// notesLog is the server's fold of the notes log, made on first use.
func (s *server) notesLog() *logFold {
	s.notesLogOnce.Do(func() {
		s.notesFold = &logFold{name: notesLogName, assemble: s.assembleNotes}
	})
	return s.notesFold
}

// assembleNotes turns the fold's records into the notes document,
// through cap-notes (`log_document`). Caller holds the fold's lock.
func (s *server) assembleNotes(records map[string]json.RawMessage) error {
	s.notesDoc = logDoc{}
	raw, ok := records["note"]
	if !ok || len(raw) <= 2 || s.notesWasm == "" {
		return nil
	}
	ctx := context.Background()
	req := map[string]any{"op": "log_document", "records": raw}
	if own := ownRecord(records, "notes"); own != nil {
		req["document"] = own
	}
	var out struct {
		DocumentHash string `json:"document_hash"`
		Document     string `json:"document"`
	}
	if err := s.withNotes(ctx, func(host *WasmHost) error {
		return host.Invoke(ctx, req, &out)
	}); err != nil {
		return err
	}
	s.notesDoc = logDoc{out.DocumentHash, []byte(out.Document)}
	return nil
}

// notesHead is the notes document in force: its hash and bytes, or found
// false when there is none (or no store).
func (s *server) notesHead() (hash string, doc []byte, found bool, err error) {
	if s.store == nil {
		return "", nil, false, nil
	}
	f := s.notesLog()
	if _, err := s.refreshLog(f); err != nil {
		return "", nil, false, err
	}
	f.mu.Lock()
	defer f.mu.Unlock()
	return s.notesDoc.hash, s.notesDoc.doc, s.notesDoc.hash != "", nil
}

// notesLogDoc answers the notes document of this hash, if the fold holds
// it: what /blobs serves for the manifest's pin.
func (s *server) notesLogDoc(hash string) (kind string, doc []byte, ok bool) {
	if s.store == nil || s.notesFold == nil {
		return "", nil, false
	}
	f := s.notesFold
	f.mu.Lock()
	defer f.mu.Unlock()
	if hash == s.notesDoc.hash && hash != "" {
		return "notes", s.notesDoc.doc, true
	}
	return "", nil, false
}

// handleNotes serves POST /notes: one note written or edited.
func (s *server) handleNotes(w http.ResponseWriter, r *http.Request) {
	if s.store == nil {
		http.Error(w, "the notes need the store (start with -store <url>)", http.StatusNotFound)
		return
	}
	if r.Method != http.MethodPost {
		http.Error(w, "use POST (clients read the notes document through the manifest's notes_hash)", http.StatusMethodNotAllowed)
		return
	}
	var req struct {
		ID     string          `json:"id"`
		Target json.RawMessage `json:"target"`
		Text   string          `json:"text"`
	}
	body, err := io.ReadAll(io.LimitReader(r.Body, 1<<20))
	if err == nil {
		err = json.Unmarshal(body, &req)
	}
	if err != nil || req.ID == "" || len(req.Target) == 0 {
		http.Error(w, `body: {"id": "<the note's id>", "target": {...}, "text": "..."}`, http.StatusBadRequest)
		return
	}
	ctx := r.Context()
	writer := writerOf(r, "anonymous")
	f := s.notesLog()
	var notesHash string
	_, _, refusal, err := s.writeLog(f, logWrite{writer: writer, op: "POST /notes", argsHash: store.HashOf(body)}, func() ([]json.RawMessage, *logRefusal, error) {
		var records []json.RawMessage
		var refusal *logRefusal
		err := s.withNotes(ctx, func(host *WasmHost) error {
			write := map[string]any{"op": "write", "id": req.ID, "target": req.Target, "text": req.Text, "by": writer, "at": time.Now().Unix()}
			if s.notesDoc.hash != "" {
				var loaded map[string]any
				if err := host.InvokeBytes(ctx, opLoadNotes, s.notesDoc.doc, &loaded); err != nil {
					return err
				}
				write["notes_hash"] = s.notesDoc.hash
			}
			var written struct {
				Refusals  json.RawMessage `json:"refusals"`
				NotesHash *string         `json:"notes_hash"`
			}
			if err := host.Invoke(ctx, write, &written); err != nil {
				return err
			}
			if written.NotesHash == nil {
				refusal = &logRefusal{http.StatusUnprocessableEntity, map[string]any{"refusals": written.Refusals}}
				return nil
			}
			notesHash = *written.NotesHash
			var out logNext
			if err := host.Invoke(ctx, map[string]any{"op": "log_records", "notes_hash": notesHash}, &out); err != nil {
				return err
			}
			records = out.Next
			return nil
		})
		if err != nil || refusal != nil {
			return nil, refusal, err
		}
		return records, nil, nil
	})
	if err != nil {
		logWriteFailed(w, err, http.StatusBadRequest)
		return
	}
	if refusal != nil {
		writeJSON(w, refusal.status, refusal.body)
		return
	}
	f.mu.Lock()
	hash := s.notesDoc.hash
	f.mu.Unlock()
	if hash == "" {
		hash = notesHash
	}
	writeJSON(w, http.StatusOK, map[string]any{"notes_hash": hash, "id": req.ID})
}
