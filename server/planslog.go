package main

// The plans log (D-190, D-191): the record of freezing and adopting.
// cap-log keeps the log (caplog.go); cap-plans answers its records and
// the two documents assembled from them: the published plan set
// (`plans`, from `pick` records), which the manifest pins as plans_hash,
// and the frozen versions (`frozen_versions`, from `frozen_version`
// records). The host decides nothing.
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
// only, and seeds the log once.

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"

	"rustwasm/server/store"
)

// plansLogName is the plans log's name in the bucket.
const plansLogName = "plans"

// plansLog is the server's fold of the plans log, made on first use.
func (s *server) plansLog() *logFold {
	s.plansLogOnce.Do(func() {
		s.plansFold = &logFold{name: plansLogName, assemble: s.assemblePlans, documents: func() map[string]string {
			return map[string]string{"plans_hash": s.plansDoc.hash, "frozen_versions_hash": s.frozenDoc.hash}
		}}
	})
	return s.plansFold
}

// assemblePlans turns the fold's records into the published plan set and
// the frozen versions, through cap-plans (`log_document`). Caller holds
// the fold's lock.
func (s *server) assemblePlans(records map[string]json.RawMessage) error {
	s.plansDoc, s.frozenDoc = logDoc{}, logDoc{}
	for _, d := range []struct {
		record, kind string
		into         *logDoc
	}{{"pick", "plans", &s.plansDoc}, {"frozen_version", "frozen_versions", &s.frozenDoc}} {
		raw, ok := records[d.record]
		own := ownRecord(records, d.kind)
		if (!ok || len(raw) <= 2) && own == nil {
			continue
		}
		if s.plans == nil {
			return fmt.Errorf("the plans log needs the plans module (-plans-wasm)")
		}
		if !ok {
			raw = json.RawMessage("{}")
		}
		req := map[string]any{"op": "log_document", "kind": d.kind, "records": raw}
		if own != nil {
			req["document"] = own
		}
		var out struct {
			Document     string `json:"document"`
			DocumentHash string `json:"document_hash"`
		}
		if err := s.plans.Invoke(context.Background(), req, &out); err != nil {
			return err
		}
		*d.into = logDoc{out.DocumentHash, []byte(out.Document)}
	}
	return nil
}

// plansHead is the published plan set in force: its hash and bytes, or
// found false when nothing is published (or no store).
func (s *server) plansHead() (hash string, doc []byte, found bool, err error) {
	if s.store == nil || s.plans == nil {
		return "", nil, false, nil
	}
	f := s.plansLog()
	if _, err := s.refreshLog(f); err != nil {
		return "", nil, false, err
	}
	f.mu.Lock()
	defer f.mu.Unlock()
	return s.plansDoc.hash, s.plansDoc.doc, s.plansDoc.hash != "", nil
}

// plansLogDoc answers the plans log's document of this hash, if the fold
// holds it: what /blobs serves for the manifest's plans_hash.
func (s *server) plansLogDoc(hash string) (kind string, doc []byte, ok bool) {
	if s.store == nil || s.plansFold == nil {
		return "", nil, false
	}
	f := s.plansFold
	f.mu.Lock()
	defer f.mu.Unlock()
	switch hash {
	case s.plansDoc.hash:
		return "plans", s.plansDoc.doc, s.plansDoc.hash != ""
	case s.frozenDoc.hash:
		return "frozen_versions", s.frozenDoc.doc, s.frozenDoc.hash != ""
	}
	return "", nil, false
}

// orNull is a document's bytes, or JSON null when there is none.
func orNull(doc []byte) json.RawMessage {
	if len(doc) == 0 {
		return json.RawMessage("null")
	}
	return json.RawMessage(doc)
}

// planAnswer is cap-plans' answer to a write of the plans log: the next
// document, or the reasons, and the document's records.
type planAnswer struct {
	Issues       json.RawMessage   `json:"issues"`
	Document     *string           `json:"document"`
	DocumentHash *string           `json:"document_hash"`
	Next         []json.RawMessage `json:"next"`
}

// refused is the 422 of a write cap-plans refused, or nil.
func (a planAnswer) refused() *logRefusal {
	if a.Document != nil {
		return nil
	}
	return &logRefusal{http.StatusUnprocessableEntity, map[string]any{"issues": a.Issues}}
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
	seq, _, refusal, err = s.writeLog(s.plansLog(), logWrite{writer: writer, op: op, argsHash: w.pick.DocumentHash}, func() ([]json.RawMessage, *logRefusal, error) {
		var next []json.RawMessage
		frozen := orNull(s.frozenDoc.doc)
		if w.purpose != "" {
			var ans planAnswer
			if err := s.plans.Invoke(ctx, map[string]any{
				"op": "freeze", "frozen": frozen, "plan": w.pick.Plan, "network_hash": w.pick.DocumentHash, "purpose": w.purpose,
			}, &ans); err != nil {
				return nil, nil, err
			}
			if r := ans.refused(); r != nil {
				return nil, r, nil
			}
			frozen = json.RawMessage(*ans.Document)
			next = append(next, ans.Next...)
		}
		if w.adopt {
			req := map[string]any{
				"op": "adopt", "published": orNull(s.plansDoc.doc), "frozen": frozen, "network_hash": w.pick.DocumentHash, "today": s.today(),
			}
			if w.stopsSeq > 0 {
				req["stops_seq"] = w.stopsSeq
			}
			var ans planAnswer
			if err := s.plans.Invoke(ctx, req, &ans); err != nil {
				return nil, nil, err
			}
			if r := ans.refused(); r != nil {
				return nil, r, nil
			}
			next = append(next, ans.Next...)
		}
		return next, nil, nil
	})
	return seq, refusal, err
}

// stopsSeqRead is the stops log entry a pick reads: the head of the
// stops log (0 with no stops log).
func (s *server) stopsSeqRead() int64 {
	if s.store == nil || s.stopsWasm == "" {
		return 0
	}
	f := s.stopsLog()
	if _, err := s.refreshLog(f); err != nil {
		return 0
	}
	f.mu.Lock()
	defer f.mu.Unlock()
	if s.stopsDoc.hash == "" {
		return 0
	}
	return f.seq
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
	f := s.plansLog()
	f.mu.Lock()
	frozenHash := s.frozenDoc.hash
	f.mu.Unlock()
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

// seedPlansLog gives an empty plans log its first entry at startup: the
// old `plans` pointer's head, each of its picks frozen as it was
// published (writer migrate:plans, the note naming the head's hash), so
// every network in force is a frozen version. Nothing when the pointer
// does not exist. The pointer is never written again.
func (s *server) seedPlansLog() {
	if s.plans == nil {
		return
	}
	head, err := s.store.LogHeadOf(plansLogName)
	if err != nil || head.Seq > 0 || len(head.Gaps) > 0 {
		if err != nil {
			log.Printf("seed the plans log: %v", err)
		}
		return
	}
	ptr, found, err := s.store.Head("plans")
	if err != nil || !found {
		return
	}
	set, _, err := s.store.GetBlob(ptr.Hash)
	if err != nil {
		log.Printf("seed the plans log: the plans pointer's head %s: %v", ptr.Hash, err)
		return
	}
	ctx := context.Background()
	frozen := json.RawMessage("null")
	for _, p := range publishedEntries(set) {
		plan := map[string]any{"id": p.ID, "name": p.Name, "effective_from": p.EffectiveFrom}
		var ans planAnswer
		if err := s.plans.Invoke(ctx, map[string]any{
			"op": "freeze", "frozen": frozen, "plan": plan, "network_hash": p.NetworkHash, "purpose": "published before the plans log",
		}, &ans); err != nil || ans.Document == nil {
			log.Printf("seed the plans log: pick %s: %v %s", p.ID, err, ans.Issues)
			return
		}
		frozen = json.RawMessage(*ans.Document)
	}
	req := map[string]any{"op": "log_records", "published": json.RawMessage(set)}
	if string(frozen) != "null" {
		req["frozen"] = frozen
	}
	var records logNext
	if err := s.plans.Invoke(ctx, req, &records); err != nil {
		log.Printf("seed the plans log: %v", err)
		return
	}
	note := "plans " + ptr.Hash
	seq, _, _, err := s.writeLog(s.plansLog(), logWrite{writer: "migrate:plans", op: "migrate", note: note}, func() ([]json.RawMessage, *logRefusal, error) {
		return records.Next, nil, nil
	})
	if err != nil {
		log.Printf("seed the plans log: %v", err)
		return
	}
	log.Printf("seed the plans log: entry %d from %s", seq, note)
}
