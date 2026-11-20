package main

// The intent write (D-189): one write path for every log that a page
// writes through its capabilities' ops. The page sends what the person
// asked for, never the next document:
//
//	POST /logs/<log>  body: {"key": "<the device's idempotency key>",
//	                         "ops": [{"cap": "...", "op": "...", "args": {...}}],
//	                         "read_seq": N,          (optional)
//	                         "claimed_at_ms": T}     (optional)
//	                  200 {seq, <each document's hash>, landed?}
//	                  422 {op, refusals}
//	                  409 {op, conflict} | {conflict, overlap: {kind, id, seq}, seq}
//
// The server runs each op over the documents at the log's head, in
// order, so the module's checks run on what the server holds, and
// writes what the ops changed as one entry (writeLog). A key that landed
// answers the entry that holds it, so a resend lands once. `read_seq` is
// the log's seq whose documents the page read: a record the write
// changes that a later entry changed is a 409 for a person to settle,
// and a change to another record does not refuse it. `claimed_at_ms` is
// the device's clock when the person made the write, kept in the
// entry's intent beside the server's stamp (D-222).
//
// The stops log and its ops: stops add_event, correct_event; requests
// put, record_work_order. The write rule of each op is its document's
// pointer (mayWrite). A log a receive op writes takes its intent writes
// through logkinds.go.

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"sort"
	"strings"

	"rustwasm/server/store"
)

// opsWrite is one op of an intent write.
type opsWrite struct {
	Cap  string          `json:"cap"`
	Op   string          `json:"op"`
	Args json.RawMessage `json:"args"`
}

// intentLog is one log's intent write: the ops it runs, each with the
// pointer whose write rule it needs, how it runs them over the head,
// and the hashes it answers.
type intentLog struct {
	fold func() *logFold
	// ops maps a capability to its ops, each to its pointer.
	ops map[string]map[string]string
	// run runs the ops over the documents at the head (the fold's lock
	// held) and answers the records of each document they changed.
	run func(ctx context.Context, r *http.Request, ops []opsWrite) ([]json.RawMessage, *logRefusal, error)
	// answer names each document's hash (the fold's lock held).
	answer func() map[string]any
}

// intentLogs are the logs a page writes by intent, by name.
func (s *server) intentLogs() map[string]intentLog {
	return map[string]intentLog{
		stopsLogName: {
			fold: s.stopsLog,
			ops: map[string]map[string]string{
				"stops":    {"add_event": stopsPointer, "correct_event": stopsPointer},
				"requests": {"put": requestsPointer, "record_work_order": requestsPointer},
			},
			run: func(ctx context.Context, r *http.Request, ops []opsWrite) ([]json.RawMessage, *logRefusal, error) {
				return s.runStopsOps(ctx, ops, nameOf(r, "anonymous"))
			},
			answer: func() map[string]any {
				return map[string]any{"stops_hash": s.stopsDoc.hash, "stop_requests_hash": s.requestsDoc.hash}
			},
		},
	}
}

// intentUsage names a log's ops, for a 400.
func (l intentLog) usage() string {
	var parts []string
	for c, ops := range l.ops {
		var names []string
		for op := range ops {
			names = append(names, op)
		}
		sort.Strings(names)
		parts = append(parts, c+": "+strings.Join(names, ", "))
	}
	sort.Strings(parts)
	return strings.Join(parts, "; ")
}

// handleIntentWrite is POST /logs/<log> for a log a page writes by
// intent: each op run over the documents at the head, in order, and
// what they changed written as one entry.
func (s *server) handleIntentWrite(w http.ResponseWriter, r *http.Request, spec intentLog) {
	body, err := io.ReadAll(io.LimitReader(r.Body, 32<<20))
	var req struct {
		Key         string     `json:"key"`
		Ops         []opsWrite `json:"ops"`
		ReadSeq     *int64     `json:"read_seq"`
		ClaimedAtMs int64      `json:"claimed_at_ms"`
	}
	if err == nil {
		err = json.Unmarshal(body, &req)
	}
	if err != nil || len(req.Ops) == 0 {
		http.Error(w, `body: {"key": "<idempotency key>", "ops": [{"cap": "...", "op": "...", "args": {...}}], "read_seq": N, "claimed_at_ms": T}`, http.StatusBadRequest)
		return
	}
	f := spec.fold()
	for i, op := range req.Ops {
		pointer, ok := spec.ops[op.Cap][op.Op]
		if !ok {
			http.Error(w, fmt.Sprintf("op %d: %s %s is not a write POST /logs/%s runs (%s)", i, op.Cap, op.Op, f.name, spec.usage()), http.StatusBadRequest)
			return
		}
		if allow, reason := s.mayWrite(r, pointer); !allow {
			http.Error(w, "forbidden: "+reason, http.StatusForbidden)
			return
		}
	}
	ctx := r.Context()
	write := logWrite{writer: writerOf(r, "anonymous"), op: "POST /logs/" + f.name, argsHash: store.HashOf(body), key: req.Key, readSeq: req.ReadSeq, claimedAtMs: req.ClaimedAtMs}
	seq, landed, refusal, err := s.writeLog(f, write, func() ([]json.RawMessage, *logRefusal, error) {
		return spec.run(ctx, r, req.Ops)
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
	answer := spec.answer()
	f.mu.Unlock()
	answer["seq"] = seq
	if landed {
		answer["landed"] = true
	}
	writeJSON(w, http.StatusOK, answer)
}

// opArgs is an op's arguments as a map with its name, or the 400 that
// names it.
func opArgs(i int, op opsWrite) (map[string]any, *logRefusal) {
	var args map[string]any
	if len(op.Args) > 0 {
		if err := json.Unmarshal(op.Args, &args); err != nil {
			return nil, &logRefusal{http.StatusBadRequest, map[string]any{"op": i, "error": "args: " + err.Error()}}
		}
	}
	if args == nil {
		args = map[string]any{}
	}
	args["op"] = op.Op
	return args, nil
}
