package main

// The plans log (D-190, D-191). Its receive op writes the published plan
// set (`plans`), which the manifest pins as plans_hash, and the frozen
// versions (`frozen_versions`). The routes are in server/openapi.json.
//
// A frozen network is the head of its plan's plan.<id> pointer, so the
// store keeps every version. The old `plans` pointer stays in the
// bucket, read only, and seeds the log once (seedKept). A page never
// writes the plans log by intent, because a freeze stores its network
// first.

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"

	"rustwasm/server/store"
	"rustwasm/server/wasmhost"
)

func orNull(doc []byte) json.RawMessage {
	if len(doc) == 0 {
		return json.RawMessage("null")
	}
	return json.RawMessage(doc)
}

// warnings holds the network's warnings, then the stops it visits that
// are not in service. unknownFields names the fields that this build
// does not know (D-167).
type pickWritten struct {
	planID        string
	networkHash   string
	seq           int64
	warnings      json.RawMessage
	unknownFields []string
}

// writePick writes one `pick` op of cap-plans' receive (D-237). It
// stores the network as the head of plan.<id> before the entry, so a
// frozen version is always stored. The routes checked the write rule.
// An error is the server's fault, and a refusal is the writer's.
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
	// A migration names the version it came from, and the frozen version
	// pins it (D-232).
	if from != "" {
		args["from_network_hash"] = from
	}
	loads := []hostLoad{{opLoadNetwork, body}}
	if e := s.pickKind(); adopt && e != nil {
		// The adoption names the entry of the kind's log that it read (D-191).
		if seq := s.keptSeq(e.StoreKind); seq > 0 {
			args[e.Pick.Seq] = seq
		}
		_, doc, found, err := s.keptHead(e.StoreKind)
		if err != nil {
			return out, nil, err
		}
		if found {
			loads = append(loads, hostLoad{e.Load.Code, doc})
		}
	}
	seq, _, got, refusal, err := s.receiveLog(ctx, nil, l, logWrite{writer: writer, op: op, argsHash: store.HashOf(body)}, receiveCall{
		loads: loads,
		onLoad: func(answer map[string]any) {
			if _, network := answer["network_hash"]; !network {
				return
			}
			// today below is a date in the agency's zone.
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
		return out, nil, &wasmhost.Refused{ModuleError: got.Failed.Error}
	case got.Refused != nil:
		// The 422 body is {issues, refusals} when the network does not
		// publish, and {issues} for a plan issue.
		return out, &publishRefusal{code: http.StatusUnprocessableEntity, json: got.Refused.Refusals}, nil
	case refusal != nil:
		if m, ok := refusal.body.(map[string]any); ok && refusal.status == http.StatusBadRequest {
			if e, ok := m["error"].(wasmhost.ModuleError); ok {
				if e.Code != "" {
					body, _ := json.Marshal(map[string]wasmhost.ModuleError{"error": e})
					return out, &publishRefusal{code: refusal.status, json: body}, nil
				}
				return out, &publishRefusal{code: refusal.status, text: e.Message}, nil
			}
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

// receivePlans needs no write check, because the routes made it (the
// policy's `plans`). A refusal is the 422 of the plans' issues.
func (s *server) receivePlans(ctx context.Context, l *keptLog, w logWrite, ops []json.RawMessage) (int64, *logRefusal, error) {
	seq, _, got, refusal, err := s.receiveLog(ctx, nil, l, w, receiveCall{ops: ops})
	switch {
	case err != nil:
		return 0, nil, err
	case got.Failed != nil:
		return 0, nil, &wasmhost.Refused{ModuleError: got.Failed.Error}
	case got.Refused != nil:
		return 0, &logRefusal{http.StatusUnprocessableEntity, map[string]any{"issues": got.Refused.Refusals}}, nil
	}
	return seq, refusal, nil
}

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
