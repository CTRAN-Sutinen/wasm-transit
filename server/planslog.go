package main

// The plans log (D-190, D-191): the record of freezing and adopting.
// Its receive op writes it (logkinds.go; cap-plans `receive`): the
// published plan set (`plans`, from `pick` records), which the manifest
// pins as plans_hash, and the frozen versions (`frozen_versions`, from
// `frozen_version` records). The host decides nothing.
//
// Every write is one entry:
//
//	POST /plans/freeze?purpose=<what it is>   body: a network document
//	                  200 {plan_id, network_hash, frozen_versions_hash, seq, warnings}
//	POST /plans/adopt  body: {"network_hash": "<a frozen version>"}
//	                  200 {plan_id, network_hash, plans_hash, seq, warnings}
//	POST /plans        the freeze and the adoption in one entry (plans.go)
//	POST /plans/retire a pick that has not taken effect (plans.go)
//
// A frozen network is the head of its plan's plan.<id> pointer when it
// is frozen, so the store keeps every version. The documents are served
// by hash from /blobs. The old `plans` pointer stays in the bucket, read
// only, and seeds the log once (seedKept). A page never writes the plans
// log by intent: a freeze stores its network first.

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"

	"rustwasm/server/store"
)

// orNull is a document's bytes, or JSON null when there is none.
func orNull(doc []byte) json.RawMessage {
	if len(doc) == 0 {
		return json.RawMessage("null")
	}
	return json.RawMessage(doc)
}

// planWrite is one write of a pick to the plans log: a freeze, an
// adoption, or both in one entry.
type planWrite struct {
	pick    canonicalNetwork
	purpose string // the freeze's; "" = no freeze
	adopt   bool
	// stopsSeq is the stops log entry the adoption read (0: none).
	stopsSeq int64
}

// writePlan runs a freeze, an adoption, or both through cap-plans over
// the plans log's head, as one entry. The network is the head of its
// plan.<id> pointer first, so a frozen version is always stored.
func (s *server) writePlan(ctx context.Context, w planWrite, writer, op string) (seq int64, refusal *logRefusal, err error) {
	var frame struct {
		ID string `json:"id"`
	}
	json.Unmarshal(w.pick.Plan, &frame)
	if w.purpose != "" {
		pointer := "plan." + frame.ID
		if !store.ValidPointerName(pointer) {
			return 0, &logRefusal{http.StatusBadRequest, map[string]any{"error": fmt.Sprintf("plan id %q cannot name a pointer (letters, digits, '.', '_', '-')", frame.ID)}}, nil
		}
		env := store.Envelope{Kind: "network_document", SchemaVersion: networkVersion, ProducedBy: w.pick.ProducedBy}
		if err := commitHead(s.store, pointer, []byte(w.pick.Document), env, w.pick.DocumentHash, writer); err != nil {
			return 0, nil, err
		}
	}
	l := s.keptLogOf("plans")
	if l == nil {
		return 0, nil, fmt.Errorf("the kind table names no log for the plans")
	}
	var ops []json.RawMessage
	if w.purpose != "" {
		op, _ := json.Marshal(map[string]any{"cap": "plans", "op": "freeze", "args": map[string]any{"plan": w.pick.Plan, "network_hash": w.pick.DocumentHash, "purpose": w.purpose}})
		ops = append(ops, op)
	}
	if w.adopt {
		args := map[string]any{"network_hash": w.pick.DocumentHash, "today": s.today()}
		if w.stopsSeq > 0 {
			args["stops_seq"] = w.stopsSeq
		}
		op, _ := json.Marshal(map[string]any{"cap": "plans", "op": "adopt", "args": args})
		ops = append(ops, op)
	}
	return s.receivePlans(ctx, l, logWrite{writer: writer, op: op, argsHash: w.pick.DocumentHash}, ops)
}

// receivePlans writes the plans log through its receive op. The routes
// checked the write rule (the policy's `plans`), so none is asked here.
// A refusal is the 422 of the plans' issues, and an op that did not run
// is an error.
func (s *server) receivePlans(ctx context.Context, l *keptLog, w logWrite, ops []json.RawMessage) (int64, *logRefusal, error) {
	seq, _, got, refusal, err := s.receiveLog(ctx, nil, l, w, receiveCall{ops: ops})
	switch {
	case err != nil:
		return 0, nil, err
	case got.Failed != nil:
		return 0, nil, errors.New(got.Failed.Error)
	case got.Refused != nil:
		return 0, &logRefusal{http.StatusUnprocessableEntity, map[string]any{"issues": got.Refused.Refusals}}, nil
	}
	return seq, refusal, nil
}

// stopsSeqRead is the stops log entry a pick reads: the head of the
// stops log (0 with no stops log).
func (s *server) stopsSeqRead() int64 {
	if s.store == nil {
		return 0
	}
	return s.keptSeq("stops")
}

// handleFreeze is POST /plans/freeze: the network written to the record
// with what it is, and nothing put in force.
func (s *server) handleFreeze(w http.ResponseWriter, r *http.Request) {
	if !s.plansWriteReady(w, r) {
		return
	}
	purpose := r.URL.Query().Get("purpose")
	if purpose == "" {
		http.Error(w, "purpose: what the version is (?purpose=proposal for public comment)", http.StatusBadRequest)
		return
	}
	body, err := io.ReadAll(io.LimitReader(r.Body, 128<<20))
	if err != nil {
		http.Error(w, "read body: "+err.Error(), http.StatusBadRequest)
		return
	}
	pick, issues, err := canonicalPick(r.Context(), s.networkWasm, body)
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	if issues != nil {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusUnprocessableEntity)
		w.Write(issues)
		return
	}
	writer := writerOf(r, "anonymous")
	seq, refusal, err := s.writePlan(r.Context(), planWrite{pick: pick, purpose: purpose}, writer, "POST /plans/freeze")
	if err != nil {
		logWriteFailed(w, err, http.StatusInternalServerError)
		return
	}
	if refusal != nil {
		if m, ok := refusal.body.(map[string]any); ok && refusal.status == http.StatusBadRequest {
			http.Error(w, fmt.Sprint(m["error"]), refusal.status)
			return
		}
		writeJSON(w, refusal.status, refusal.body)
		return
	}
	flagUnknown(w, "plans", writer, pick.UnknownFields)
	frozenHash, _, _, _ := s.keptHead("frozen_versions")
	writeJSON(w, http.StatusOK, map[string]any{
		"plan_id": planID(pick), "network_hash": pick.DocumentHash, "frozen_versions_hash": frozenHash, "seq": seq, "warnings": pick.Warnings,
	})
}

// handleAdopt is POST /plans/adopt: a frozen version put in force from
// its date.
func (s *server) handleAdopt(w http.ResponseWriter, r *http.Request) {
	if !s.plansWriteReady(w, r) {
		return
	}
	var req struct {
		NetworkHash string `json:"network_hash"`
	}
	if err := json.NewDecoder(io.LimitReader(r.Body, 1<<16)).Decode(&req); err != nil || req.NetworkHash == "" {
		http.Error(w, `body: {"network_hash": "<a frozen version's network>"}`, http.StatusBadRequest)
		return
	}
	if _, err := store.ParseHash(req.NetworkHash); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	body, _, err := s.store.GetBlob(req.NetworkHash)
	if err == store.ErrNotFound {
		writeJSON(w, http.StatusUnprocessableEntity, map[string]any{"issues": []any{map[string]any{"kind": "not_frozen", "network_hash": req.NetworkHash}}})
		return
	}
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return
	}
	out, refused := s.adoptNetwork(r.Context(), body, "", writerOf(r, "anonymous"), "POST /plans/adopt")
	if refused != nil {
		refused.write(w)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{
		"plan_id": out.planID, "network_hash": out.networkHash, "plans_hash": out.plansHash, "seq": out.seq, "warnings": out.warnings,
	})
}

// plansWriteReady answers a write the plans log cannot take here: no
// store, no plans module, or not a POST.
func (s *server) plansWriteReady(w http.ResponseWriter, r *http.Request) bool {
	if s.store == nil || s.plans == nil {
		http.Error(w, "publishing needs the store (start with -store <url>)", http.StatusNotFound)
		return false
	}
	if r.Method != http.MethodPost {
		http.Error(w, "use POST", http.StatusMethodNotAllowed)
		return false
	}
	return true
}

func planID(pick canonicalNetwork) string {
	var frame struct {
		ID string `json:"id"`
	}
	json.Unmarshal(pick.Plan, &frame)
	return frame.ID
}
