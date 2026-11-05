package main

// Publishing a pick (store mode): POST /plans takes a plan's network
// document — any serialization, gzipped or not — and makes it the
// pick every client resolves (docs/ROADMAP.md 1). The host decides
// nothing:
//
//   - cap-network (a fresh instance, closed after) registers the
//     document, validates it (a network with issues never publishes),
//     and answers its canonical bytes, hash and plan frame;
//   - cap-plans answers the next published plan set — or the reasons
//     the pick would break the chain — as canonical bytes and hash.
//
// The host stores exactly those bytes: the network under the
// plan.<id> pointer (its log keeps every revision referenced, so GC
// never collects a published network), then the set under the plans
// pointer, which the manifest pins as plans_hash. A client reads the
// set by hash, asks cap-plans in_force for the date, and loads that
// network by hash from /blobs.
//
//	POST /plans   body: the network document
//	              200 {plan_id, network_hash, plans_hash, warnings}
//	              422 {issues} (invalid network, or a pick the chain refuses)
//	              400 (not a network; a plan id that is not a pointer name)

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"

	"rustwasm/server/store"
)

// plansEnvelope is the envelope of every published plan set: produced
// by the hosted cap-plans module.
func (s *server) plansEnvelope() store.Envelope {
	return store.Envelope{Kind: "plans", SchemaVersion: 1, ProducedBy: "cap-plans@" + s.plans.Hash()}
}

// canonicalPick runs a received network through a fresh cap-network:
// register (kernel op 256; gzip or plain), validate, and the canonical
// document with its plan frame. The instance closes with the call.
func canonicalPick(ctx context.Context, wasmPath string, body []byte) (doc canonicalNetwork, issues json.RawMessage, err error) {
	host, err := NewWasmHost(ctx, wasmPath)
	if err != nil {
		return doc, nil, fmt.Errorf("network module: %w", err)
	}
	defer host.Close(ctx)
	var loaded struct {
		NetworkHash string `json:"network_hash"`
	}
	if err := host.InvokeBytes(ctx, opLoadNetwork, body, &loaded); err != nil {
		return doc, nil, fmt.Errorf("not a network document: %w", err)
	}
	var checked struct {
		Issues   json.RawMessage `json:"issues"`
		Warnings json.RawMessage `json:"warnings"`
	}
	if err := host.Invoke(ctx, map[string]any{"op": "validate", "network_hash": loaded.NetworkHash}, &checked); err != nil {
		return doc, nil, err
	}
	if len(checked.Issues) > 2 { // anything but []
		return doc, checked.Issues, nil
	}
	if err := host.Invoke(ctx, map[string]any{"op": "document", "network_hash": loaded.NetworkHash}, &doc); err != nil {
		return doc, nil, err
	}
	doc.Warnings = checked.Warnings
	doc.ProducedBy = "cap-network@" + host.Hash()
	return doc, nil, nil
}

// canonicalNetwork is cap-network's document answer: the bytes, their
// hash, and the plan frame (passed through to cap-plans unread).
type canonicalNetwork struct {
	Document     string          `json:"document"`
	DocumentHash string          `json:"document_hash"`
	Plan         json.RawMessage `json:"plan"`
	Warnings     json.RawMessage `json:"-"`
	ProducedBy   string          `json:"-"`
}

func (s *server) handlePlans(w http.ResponseWriter, r *http.Request) {
	if s.store == nil || s.plans == nil {
		http.Error(w, "publishing needs the store (start with -store <dir>)", http.StatusNotFound)
		return
	}
	if r.Method != http.MethodPost {
		http.Error(w, "use POST (clients read the published set through the manifest's plans_hash)", http.StatusMethodNotAllowed)
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
		writeJSON(w, http.StatusUnprocessableEntity, map[string]any{"issues": issues})
		return
	}
	var frame struct {
		ID string `json:"id"`
	}
	json.Unmarshal(pick.Plan, &frame)

	// One publish at a time: the set is read, extended and committed
	// as one step (the CAS below still guards against any other writer).
	s.plansMu.Lock()
	defer s.plansMu.Unlock()
	head, found, err := s.store.Head("plans")
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	var current json.RawMessage = json.RawMessage("null")
	if found {
		if current, _, err = s.store.GetBlob(head.Hash); err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
	}
	var next struct {
		Issues       json.RawMessage `json:"issues"`
		Document     *string         `json:"document"`
		DocumentHash *string         `json:"document_hash"`
	}
	if err := s.plans.Invoke(r.Context(), map[string]any{
		"op": "publish", "published": current, "plan": pick.Plan, "network_hash": pick.DocumentHash,
	}, &next); err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	if next.Document == nil {
		writeJSON(w, http.StatusUnprocessableEntity, map[string]any{"issues": next.Issues})
		return
	}
	pointer := "plan." + frame.ID
	if !store.ValidPointerName(pointer) {
		http.Error(w, fmt.Sprintf("plan id %q cannot name a pointer (letters, digits, '.', '_', '-')", frame.ID), http.StatusBadRequest)
		return
	}

	writer := writerOf(r, "anonymous")
	nowMs := time.Now().UnixMilli()
	// The network first: once it is committed it is referenced, and a
	// crash before the set's commit leaves the old set in force.
	netEnv := store.Envelope{Kind: "network_document", SchemaVersion: 1, ProducedBy: pick.ProducedBy}
	planHead, planFound, _ := s.store.Head(pointer)
	planFrom := ""
	if planFound {
		planFrom = planHead.Hash
	}
	if _, _, err := s.store.CAS(pointer, planFrom, []byte(pick.Document), netEnv, pick.DocumentHash, writer, nowMs); err != nil {
		http.Error(w, err.Error(), http.StatusConflict)
		return
	}
	from := ""
	if found {
		from = head.Hash
	}
	plansHash, _, err := s.store.CAS("plans", from, []byte(*next.Document), s.plansEnvelope(), *next.DocumentHash, writer, nowMs)
	if err != nil {
		http.Error(w, err.Error(), http.StatusConflict)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{
		"plan_id": frame.ID, "network_hash": pick.DocumentHash, "plans_hash": plansHash, "warnings": pick.Warnings,
	})
}

func writeJSON(w http.ResponseWriter, code int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(code)
	json.NewEncoder(w).Encode(v)
}
