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

// pickWritten is what a pick's write answers: the pick's plan id and
// network, the entry's seq, the warnings (the network's, then the stops
// it visits that are not in service), and the fields the network
// carries that this build does not know (D-167).
type pickWritten struct {
	planID        string
	networkHash   string
	seq           int64
	warnings      json.RawMessage
	unknownFields []string
}

// writePick writes a received network to the plans log as a pick, in one
// op of cap-plans' receive (`pick`, D-237): the network's publish check,
// the stops it reads when adopted (D-157), the freeze under purpose (""
// = none), and the adoption. The module registers the network and the
// stops document on its kernel loads. The network is stored as the head
// of plan.<id> before the entry, so a frozen version is always stored.
// The routes checked the write rule, so none is asked here. An error is
// the server's; a refusal is the writer's.
func (s *server) writePick(ctx context.Context, body []byte, purpose, from string, adopt bool, writer, op string) (pickWritten, *publishRefusal, error) {
	var out pickWritten
	l := s.keptLogOf("plans")
	if l == nil {
		return out, nil, fmt.Errorf("the kind table names no log for the plans")
	}
	args := map[string]any{"adopt": adopt}
	if purpose != "" {
		args["purpose"] = purpose
	}
	// A migration that brings a pick forward names the version it came
	// from, and the frozen version pins it (D-232).
	if from != "" {
		args["from_network_hash"] = from
	}
	loads := []hostLoad{{opLoadNetwork, body}}
	if adopt {
		// The adoption names the stops log entry it read (D-191).
		if seq := s.stopsSeqRead(); seq > 0 {
			args["stops_seq"] = seq
		}
		_, stops, found, err := s.keptHead("stops")
		if err != nil {
			return out, nil, err
		}
		if found {
			loads = append(loads, hostLoad{opLoadStops, stops})
		}
	}
	seq, _, got, refusal, err := s.receiveLog(ctx, nil, l, logWrite{writer: writer, op: op, argsHash: store.HashOf(body)}, receiveCall{
		loads: loads,
		onLoad: func(answer map[string]any) {
			if _, network := answer["network_hash"]; !network {
				return
			}
			// The pick names the agency's zone: today below is its date.
			if zone, _ := answer["timezone"].(string); zone != "" {
				s.zoneMu.Lock()
				s.zone = zone
				s.zoneMu.Unlock()
			}
			fields, _ := answer["unknown_fields"].([]any)
			for _, f := range fields {
				out.unknownFields = append(out.unknownFields, fmt.Sprint(f))
			}
		},
		opsAfterLoads: func() []json.RawMessage {
			args["today"] = s.today()
			call, _ := json.Marshal(map[string]any{"cap": "plans", "op": "pick", "args": args})
			return []json.RawMessage{call}
		},
	})
	var refusedLoad *loadError
	switch {
	case errors.As(err, &refusedLoad):
		return out, refusedWith(http.StatusBadRequest, fmt.Errorf("not a network document: %w", refusedLoad.err)), nil
	case err != nil:
		return out, nil, err
	case got.Failed != nil:
		return out, nil, errors.New(got.Failed.Error)
	case got.Refused != nil:
		// The pick op's refusal is the 422 body: {issues, refusals} when
		// the network does not publish, {issues} for a plan issue.
		return out, &publishRefusal{code: http.StatusUnprocessableEntity, json: got.Refused.Refusals}, nil
	case refusal != nil:
		if m, ok := refusal.body.(map[string]any); ok && refusal.status == http.StatusBadRequest {
			return out, &publishRefusal{code: refusal.status, text: fmt.Sprint(m["error"])}, nil
		}
		body, _ := json.Marshal(refusal.body)
		return out, &publishRefusal{code: refusal.status, json: body}, nil
	}
	var answer struct {
		PlanID       string            `json:"plan_id"`
		NetworkHash  string            `json:"network_hash"`
		Warnings     json.RawMessage   `json:"warnings"`
		StopWarnings []json.RawMessage `json:"stop_warnings"`
	}
	if err := json.Unmarshal(got.Answer, &answer); err != nil {
		return out, nil, fmt.Errorf("the pick's answer: %w", err)
	}
	out.planID, out.networkHash, out.seq = answer.PlanID, answer.NetworkHash, seq
	out.warnings = withWarnings(answer.Warnings, answer.StopWarnings)
	return out, nil, nil
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
	writer := writerOf(r, "anonymous")
	pick, refused, err := s.writePick(r.Context(), body, purpose, "", false, writer, "POST /plans/freeze")
	if err != nil {
		logWriteFailed(w, err, http.StatusInternalServerError)
		return
	}
	if refused != nil {
		refused.write(w)
		return
	}
	flagUnknown(w, "plans", writer, pick.unknownFields)
	frozenHash, _, _, _ := s.keptHead("frozen_versions")
	writeJSON(w, http.StatusOK, map[string]any{
		"plan_id": pick.planID, "network_hash": pick.networkHash, "frozen_versions_hash": frozenHash, "seq": pick.seq, "warnings": pick.warnings,
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
	out, refused := s.adoptNetwork(r.Context(), body, "", "", writerOf(r, "anonymous"), "POST /plans/adopt")
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
