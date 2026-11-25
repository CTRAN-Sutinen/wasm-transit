package main

// The plans log (D-190, D-191). Properties, over HTTP:
//   - a freeze writes the network by its hash and puts nothing in force;
//   - only a frozen version is adopted, and an adoption puts it in force
//     from its date;
//   - POST /plans is a freeze and an adoption in one entry;
//   - a retirement is one entry;
//   - a restarted server folds the same documents from the log;
//   - the plans log has no generic revert.

import (
	"encoding/json"
	"net/http"
	"net/url"
	"strings"
	"testing"

	"rustwasm/server/store"
)

func freezePick(t *testing.T, base string, network map[string]any, purpose string) (int, map[string]any) {
	t.Helper()
	body, _ := json.Marshal(network)
	code, _, resp := httpPost(t, base+"/plans/freeze?purpose="+url.QueryEscape(purpose), string(body))
	var out map[string]any
	if json.Unmarshal([]byte(resp), &out) != nil {
		out = map[string]any{"raw": resp}
	}
	return code, out
}

type plansEntry struct {
	Writer  string `json:"writer"`
	Changes []struct {
		Kind  string          `json:"kind"`
		ID    string          `json:"id"`
		After json.RawMessage `json:"after"`
	} `json:"changes"`
	Intent struct {
		Op string `json:"op"`
	} `json:"intent"`
}

func lastPlansEntry(t *testing.T, s *server) (int64, plansEntry) {
	t.Helper()
	head, err := s.store.LogHeadOf("plans")
	if err != nil || head.Seq == 0 {
		t.Fatalf("the plans log: %+v %v", head, err)
	}
	stored, err := s.store.Entries("plans", head.Seq-1, head.Seq)
	if err != nil || len(stored) != 1 {
		t.Fatalf("entry %d: %v", head.Seq, err)
	}
	var e plansEntry
	if err := json.Unmarshal([]byte(stored[0].Text), &e); err != nil {
		t.Fatal(err)
	}
	return head.Seq, e
}

func (e plansEntry) kinds() map[string]int {
	out := map[string]int{}
	for _, c := range e.Changes {
		out[c.Kind]++
	}
	return out
}

func TestAFreezePutsNothingInForce(t *testing.T) {
	s, base := newPlansServer(t, store.NewMemory())
	code, out := freezePick(t, base, fixtureNetwork(t, "fall-2026", "Fall 2026", "2026-09-07"), "proposal for public comment")
	if code != 200 {
		t.Fatalf("freeze: %d %v", code, out)
	}
	if _, _, found, _ := s.keptHead("plans"); found {
		t.Fatal("a frozen version is not in force: nothing is published")
	}
	if head := mustHead(t, s, "plan.fall-2026"); head != out["network_hash"] {
		t.Fatalf("the frozen network is stored under its plan's pointer: %s, want %v", head, out["network_hash"])
	}
	code, hdr, body := httpGet(t, base+"/blobs/"+out["frozen_versions_hash"].(string))
	if code != 200 || hdr.Get("X-Document-Kind") != "frozen_versions" {
		t.Fatalf("the frozen versions are served by hash: %d %v", code, hdr)
	}
	var frozen struct {
		Versions map[string]struct {
			PlanID  string `json:"plan_id"`
			Purpose string `json:"purpose"`
		} `json:"versions"`
	}
	json.Unmarshal([]byte(body), &frozen)
	v := frozen.Versions[out["network_hash"].(string)]
	if v.PlanID != "fall-2026" || v.Purpose != "proposal for public comment" {
		t.Fatalf("the version says what it is: %+v", frozen)
	}
	if _, e := lastPlansEntry(t, s); e.kinds()["frozen_version"] != 1 || e.kinds()["pick"] != 0 || e.Intent.Op != "POST /plans/freeze" {
		t.Fatalf("one entry, a frozen version and no pick: %+v", e)
	}
	if code, _, _ := httpPost(t, base+"/plans/freeze", `{}`); code != http.StatusBadRequest {
		t.Fatalf("a freeze says what it is: %d", code)
	}
}

func TestOnlyAFrozenVersionIsAdopted(t *testing.T) {
	s, base := newPlansServer(t, store.NewMemory())
	_, frozen := freezePick(t, base, fixtureNetwork(t, "fall-2026", "Fall 2026", "2026-09-07"), "board packet")
	hash := frozen["network_hash"].(string)

	other := "sha256:" + strings.Repeat("ab", 32)
	if code, _, body := httpPost(t, base+"/plans/adopt", `{"network_hash":"`+other+`"}`); code != http.StatusUnprocessableEntity || !strings.Contains(body, "not_frozen") {
		t.Fatalf("a network no one froze: %d %s", code, body)
	}
	if code, _, _ := httpPost(t, base+"/plans/adopt", `{}`); code != http.StatusBadRequest {
		t.Fatalf("an adoption names a network: %d", code)
	}
	code, _, body := httpPost(t, base+"/plans/adopt", `{"network_hash":"`+hash+`"}`)
	if code != 200 {
		t.Fatalf("adopt: %d %s", code, body)
	}
	var out map[string]any
	json.Unmarshal([]byte(body), &out)
	if out["plans_hash"] != plansHash(t, s) || manifestPlansHash(t, base) != plansHash(t, s) {
		t.Fatalf("the adoption is the set the manifest pins: %v", out)
	}
	var set publishedSet
	json.Unmarshal(plansSet(t, s), &set)
	if len(set.Plans) != 1 || set.Plans[0].NetworkHash != hash {
		t.Fatalf("the frozen version is the pick: %+v", set)
	}
	if _, e := lastPlansEntry(t, s); e.kinds()["pick"] != 1 || e.kinds()["frozen_version"] != 0 {
		t.Fatalf("an adoption changes the set, and no frozen version: %+v", e)
	}
}

func TestAPublishIsAFreezeAndAnAdoptionInOneEntry(t *testing.T) {
	bucket := store.NewMemory()
	s, base := newPlansServer(t, bucket)
	if code, out := publishPick(t, base, fixtureNetwork(t, "fall-2026", "Fall 2026", "2026-09-07")); code != 200 {
		t.Fatalf("publish: %d %v", code, out)
	}
	seq, e := lastPlansEntry(t, s)
	if seq != 1 || e.kinds()["pick"] != 1 || e.kinds()["frozen_version"] != 1 || e.Intent.Op != "POST /plans" {
		t.Fatalf("one entry with the frozen version and the pick: seq %d %+v", seq, e)
	}
	again, _ := newPlansServer(t, bucket)
	if plansHash(t, again) != plansHash(t, s) {
		t.Fatal("the fold is the record: a restart serves the same set")
	}
	if code, _, body := httpPost(t, base+"/plans/retire", `{"id":"fall-2026"}`); code != 200 {
		t.Fatalf("retire: %d %s", code, body)
	}
	if seq, e := lastPlansEntry(t, s); seq != 2 || e.kinds()["pick"] != 1 || e.Intent.Op != "POST /plans/retire" {
		t.Fatalf("the retirement's entry: seq %d %+v", seq, e)
	}
}

func TestThePlansLogIsReadAndHasNoRevert(t *testing.T) {
	s, base := newPlansServer(t, store.NewMemory())
	publishPick(t, base, fixtureNetwork(t, "fall-2026", "Fall 2026", "2026-09-07"))
	code, _, body := httpGet(t, base+"/logs/plans")
	var log struct {
		Head               int64  `json:"head"`
		PlansHash          string `json:"plans_hash"`
		FrozenVersionsHash string `json:"frozen_versions_hash"`
	}
	json.Unmarshal([]byte(body), &log)
	if code != 200 || log.Head != 1 || log.PlansHash != plansHash(t, s) || log.FrozenVersionsHash == "" {
		t.Fatalf("GET /logs/plans: %d %s", code, body)
	}
	if code, _, _ := httpPost(t, base+"/logs/plans/revert", `{"seq":1}`); code != http.StatusMethodNotAllowed {
		t.Fatalf("a pick is corrected by its own rules: %d", code)
	}
	if code, _, _ := httpPost(t, base+"/logs/plans/close-gap", `{}`); code != http.StatusBadRequest {
		t.Fatalf("close-gap names the entry and the cause: %d", code)
	}
}
