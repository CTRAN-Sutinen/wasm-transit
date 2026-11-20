package main

// The stops log (D-190, D-197): the record of the stops document and the
// stop requests document, one log because completing a request and
// adding the stop's event is one transaction. cap-log keeps the log
// (caplog.go); cap-stops answers the `stop` records and the stops
// document assembled from them, cap-requests the `stop_request` records
// and the requests document. The host decides nothing.
//
// Every write is one entry: POST /stops, POST /stop-requests, the work
// order system's answer, the feed's recorded differences, and POST
// /logs/stops, the intent write (logwrite.go): stops add_event and
// correct_event, requests put and record_work_order, each run over the
// documents at the head, in order. A key that landed answers the entry
// that holds it. The documents are served by hash from /blobs, as
// the manifest pins them (stops_hash, stop_requests_hash). The old
// `stops` and `stop_requests` pointers stay in the bucket, read only.

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
)

// stopsLogName is the stops log's name in the bucket.
const stopsLogName = "stops"

// logDoc is one document a log's fold holds: its hash and its canonical
// bytes ("" when the fold holds none of its records).
type logDoc struct {
	hash string
	doc  []byte
}

// stopsLog is the server's fold of the stops log, made on first use.
func (s *server) stopsLog() *logFold {
	s.stopsLogOnce.Do(func() {
		s.stopsFold = &logFold{name: stopsLogName, assemble: s.assembleStops}
	})
	return s.stopsFold
}

// assembleStops turns the fold's records into the stops document and the
// stop requests document, through cap-stops and cap-requests
// (`log_document`). Caller holds the fold's lock.
func (s *server) assembleStops(records map[string]json.RawMessage) error {
	ctx := context.Background()
	var out struct {
		StopsHash    string `json:"stops_hash"`
		RequestsHash string `json:"requests_hash"`
		Document     string `json:"document"`
	}
	s.stopsDoc, s.requestsDoc = logDoc{}, logDoc{}
	if raw, ok := records["stop"]; ok && len(raw) > 2 && s.stopsWasm != "" {
		req := map[string]any{"op": "log_document", "records": raw}
		if own := ownRecord(records, "stops"); own != nil {
			req["document"] = own
		}
		if err := s.withStops(ctx, func(host *WasmHost) error {
			return host.Invoke(ctx, req, &out)
		}); err != nil {
			return err
		}
		s.stopsDoc = logDoc{out.StopsHash, []byte(out.Document)}
	}
	if raw, ok := records["stop_request"]; ok && len(raw) > 2 && s.requestsWasm != "" {
		req := map[string]any{"op": "log_document", "records": raw}
		if own := ownRecord(records, "stop_requests"); own != nil {
			req["document"] = own
		}
		if err := s.withRequests(ctx, func(host *WasmHost) error {
			return host.Invoke(ctx, req, &out)
		}); err != nil {
			return err
		}
		s.requestsDoc = logDoc{out.RequestsHash, []byte(out.Document)}
	}
	return nil
}

// stopsHead is the stops document in force: its hash and bytes, or found
// false when there is none (or no store).
func (s *server) stopsHead() (hash string, doc []byte, found bool, err error) {
	if s.store == nil {
		return "", nil, false, nil
	}
	f := s.stopsLog()
	if _, err := s.refreshLog(f); err != nil {
		return "", nil, false, err
	}
	f.mu.Lock()
	defer f.mu.Unlock()
	return s.stopsDoc.hash, s.stopsDoc.doc, s.stopsDoc.hash != "", nil
}

// requestsHead is the stop requests document in force.
func (s *server) requestsHead() (hash string, doc []byte, found bool, err error) {
	if s.store == nil {
		return "", nil, false, nil
	}
	f := s.stopsLog()
	if _, err := s.refreshLog(f); err != nil {
		return "", nil, false, err
	}
	f.mu.Lock()
	defer f.mu.Unlock()
	return s.requestsDoc.hash, s.requestsDoc.doc, s.requestsDoc.hash != "", nil
}

// stopsLogDoc answers the stops log's document of this hash, if the fold
// holds it: what /blobs serves for the manifest's pins.
func (s *server) stopsLogDoc(hash string) (kind string, doc []byte, ok bool) {
	if s.store == nil || s.stopsFold == nil {
		return "", nil, false
	}
	f := s.stopsFold
	f.mu.Lock()
	defer f.mu.Unlock()
	switch hash {
	case s.stopsDoc.hash:
		return "stops", s.stopsDoc.doc, s.stopsDoc.hash != ""
	case s.requestsDoc.hash:
		return "stop_requests", s.requestsDoc.doc, s.requestsDoc.hash != ""
	}
	return "", nil, false
}

// logNext is a kind's answer of its records (log_records): what cap-log
// composes an entry from, passed on as given.
type logNext struct {
	Next []json.RawMessage `json:"next"`
}

// stopRecords answers a stops document's records (cap-stops
// log_records): its stops, and the document's own record.
func (s *server) stopRecords(ctx context.Context, host *WasmHost, doc []byte) ([]json.RawMessage, error) {
	var loaded struct {
		StopsHash string `json:"stops_hash"`
	}
	if err := host.InvokeBytes(ctx, opLoadStops, doc, &loaded); err != nil {
		return nil, err
	}
	var out logNext
	err := host.Invoke(ctx, map[string]any{"op": "log_records", "stops_hash": loaded.StopsHash}, &out)
	return out.Next, err
}

// requestRecords answers a stop requests document's records
// (cap-requests log_records): its requests, and the document's own
// record.
func (s *server) requestRecords(ctx context.Context, host *WasmHost, doc []byte) ([]json.RawMessage, error) {
	var loaded struct {
		RequestsHash string `json:"requests_hash"`
	}
	if err := host.InvokeBytes(ctx, opLoadStopRequests, doc, &loaded); err != nil {
		return nil, err
	}
	var out logNext
	err := host.Invoke(ctx, map[string]any{"op": "log_records", "requests_hash": loaded.RequestsHash}, &out)
	return out.Next, err
}

// ownRecord is a document's own record in the fold's records of its
// kind (`document`), or nil.
func ownRecord(records map[string]json.RawMessage, kind string) json.RawMessage {
	var byID map[string]json.RawMessage
	if raw, ok := records[kind]; ok {
		json.Unmarshal(raw, &byID)
	}
	return byID["document"]
}

// seedStopsLog gives an empty stops log its first entry at startup: the
// old `stops` and `stop_requests` pointers' heads (writer migrate:stops,
// the note naming each head's hash). Nothing when neither exists. The
// pointers are never written again.
func (s *server) seedStopsLog() {
	head, err := s.store.LogHeadOf(stopsLogName)
	if err != nil || head.Seq > 0 || len(head.Gaps) > 0 {
		if err != nil {
			log.Printf("seed the stops log: %v", err)
		}
		return
	}
	ctx := context.Background()
	var next []json.RawMessage
	note := ""
	for _, p := range []struct {
		pointer string
		records func(context.Context, *WasmHost, []byte) ([]json.RawMessage, error)
		with    func(context.Context, func(*WasmHost) error) error
	}{{stopsPointer, s.stopRecords, s.withStops}, {requestsPointer, s.requestRecords, s.withRequests}} {
		ptr, found, err := s.store.Head(p.pointer)
		if err != nil || !found {
			continue
		}
		if (p.pointer == stopsPointer && s.stopsWasm == "") || (p.pointer == requestsPointer && s.requestsWasm == "") {
			log.Printf("seed the stops log: the %s pointer needs its module (-stops-wasm, -requests-wasm)", p.pointer)
			return
		}
		doc, _, err := s.store.GetBlob(ptr.Hash)
		if err != nil {
			log.Printf("seed the stops log: the %s pointer's head %s: %v", p.pointer, ptr.Hash, err)
			return
		}
		var records []json.RawMessage
		if err := p.with(ctx, func(host *WasmHost) error {
			records, err = p.records(ctx, host, doc)
			return err
		}); err != nil {
			log.Printf("seed the stops log: the %s pointer's head: %v", p.pointer, err)
			return
		}
		next = append(next, records...)
		if note != "" {
			note += ", "
		}
		note += p.pointer + " " + ptr.Hash
	}
	if len(next) == 0 {
		return
	}
	seq, _, _, err := s.writeLog(s.stopsLog(), logWrite{writer: "migrate:stops", op: "migrate", note: note}, func() ([]json.RawMessage, *logRefusal, error) {
		return next, nil, nil
	})
	if err != nil {
		log.Printf("seed the stops log: %v", err)
		return
	}
	log.Printf("seed the stops log: entry %d from %s", seq, note)
}

// fromMoved is the 409 of a whole-document write whose `from` is not the
// head: the writer reads again.
func fromMoved(from string, held logDoc, seq int64) *logRefusal {
	if from == held.hash || (from == "" && held.hash == "") {
		return nil
	}
	return &logRefusal{http.StatusConflict, map[string]any{"current": held.hash, "seq": seq}}
}

// runStopsOps runs the ops over the documents at the head (the fold's
// lock held) and answers the records of each document they changed,
// checked as a write is: the surveys named for the writer, then
// validate. A refusal or a conflict names its op.
func (s *server) runStopsOps(ctx context.Context, ops []opsWrite, by string) ([]json.RawMessage, *logRefusal, error) {
	stops, requests := s.stopsDoc.doc, s.requestsDoc.doc
	held := stops
	touched := map[string]bool{}
	for i, op := range ops {
		var args map[string]any
		if len(op.Args) > 0 {
			if err := json.Unmarshal(op.Args, &args); err != nil {
				return nil, &logRefusal{http.StatusBadRequest, map[string]any{"op": i, "error": "args: " + err.Error()}}, nil
			}
		}
		if args == nil {
			args = map[string]any{}
		}
		args["op"] = op.Op
		var answer struct {
			Refusals json.RawMessage `json:"refusals"`
			Conflict *string         `json:"conflict"`
			Stops    json.RawMessage `json:"stops"`
			Requests json.RawMessage `json:"requests"`
		}
		var err error
		switch op.Cap {
		case "stops":
			err = s.withStops(ctx, func(host *WasmHost) error {
				var loaded struct {
					StopsHash string `json:"stops_hash"`
				}
				if len(stops) == 0 {
					return fmt.Errorf("no stops document yet")
				}
				if err := host.InvokeBytes(ctx, opLoadStops, stops, &loaded); err != nil {
					return err
				}
				args["stops_hash"] = loaded.StopsHash
				return host.Invoke(ctx, args, &answer)
			})
		case "requests":
			err = s.withRequests(ctx, func(host *WasmHost) error {
				if len(requests) > 0 {
					var loaded struct {
						RequestsHash string `json:"requests_hash"`
					}
					if err := host.InvokeBytes(ctx, opLoadStopRequests, requests, &loaded); err != nil {
						return err
					}
					args["requests_hash"] = loaded.RequestsHash
				}
				return host.Invoke(ctx, args, &answer)
			})
		}
		if err != nil {
			return nil, &logRefusal{http.StatusBadRequest, map[string]any{"op": i, "error": err.Error()}}, nil
		}
		if answer.Conflict != nil {
			return nil, &logRefusal{http.StatusConflict, map[string]any{"op": i, "conflict": *answer.Conflict}}, nil
		}
		if len(answer.Refusals) > 2 {
			return nil, &logRefusal{http.StatusUnprocessableEntity, map[string]any{"op": i, "refusals": answer.Refusals}}, nil
		}
		if op.Cap == "stops" {
			stops = answer.Stops
		} else {
			requests = answer.Requests
		}
		touched[op.Cap] = true
	}
	var next []json.RawMessage
	if touched["stops"] {
		var records []json.RawMessage
		var refusal *logRefusal
		err := s.withStops(ctx, func(host *WasmHost) error {
			checked, _, ref, err := s.checkStops(ctx, host, stops, held, by)
			if err != nil || ref != nil {
				refusal = ref
				return err
			}
			records, err = s.stopRecords(ctx, host, checked)
			return err
		})
		if err != nil || refusal != nil {
			return nil, refusal, err
		}
		next = append(next, records...)
	}
	if touched["requests"] {
		var records []json.RawMessage
		var refusal *logRefusal
		err := s.withRequests(ctx, func(host *WasmHost) error {
			var checked struct {
				Refusals json.RawMessage `json:"refusals"`
				Document *string         `json:"document"`
			}
			if err := host.Invoke(ctx, map[string]any{"op": "validate", "requests": json.RawMessage(requests)}, &checked); err != nil {
				return err
			}
			if checked.Document == nil {
				refusal = &logRefusal{http.StatusUnprocessableEntity, map[string]any{"refusals": checked.Refusals}}
				return nil
			}
			var err error
			records, err = s.requestRecords(ctx, host, []byte(*checked.Document))
			return err
		})
		if err != nil || refusal != nil {
			return nil, refusal, err
		}
		next = append(next, records...)
	}
	return next, nil, nil
}

// checkStops is a stops write's receive path: every survey the held
// document does not hold names the signed-in writer (D-158; cap-stops
// attribute_surveys), then the document is checked (validate). The
// answer is the checked document's canonical bytes and the fields it
// carries that this build does not know (D-167), or the refusal.
func (s *server) checkStops(ctx context.Context, host *WasmHost, doc, held []byte, by string) ([]byte, []string, *logRefusal, error) {
	attribute := map[string]any{"op": "attribute_surveys", "stops": json.RawMessage(doc), "by": by}
	if len(held) > 0 {
		var loaded struct {
			StopsHash string `json:"stops_hash"`
		}
		if err := host.InvokeBytes(ctx, opLoadStops, held, &loaded); err != nil {
			return nil, nil, nil, fmt.Errorf("the stops document held: %w", err)
		}
		attribute["stops_hash"] = loaded.StopsHash
	}
	var attributed struct {
		Stops json.RawMessage `json:"stops"`
	}
	if err := host.Invoke(ctx, attribute, &attributed); err != nil {
		return nil, nil, nil, err
	}
	var checked struct {
		Refusals      json.RawMessage `json:"refusals"`
		Document      *string         `json:"document"`
		UnknownFields []string        `json:"unknown_fields"`
	}
	if err := host.Invoke(ctx, map[string]any{"op": "validate", "stops": attributed.Stops}, &checked); err != nil {
		return nil, nil, nil, err
	}
	if checked.Document == nil {
		return nil, nil, &logRefusal{http.StatusUnprocessableEntity, map[string]any{"refusals": checked.Refusals}}, nil
	}
	return []byte(*checked.Document), checked.UnknownFields, nil, nil
}
