package main

// Document versions at the server's receive boundaries (docs/FORMATS.md,
// "Versions"). An older POST comes forward through cap-migrate before the
// merge. A store with an older head commits it forward as a logged CAS,
// and the older blob stays readable. A pick that stopped governing stays
// in its version.

import (
	"context"
	"encoding/json"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"rustwasm/server/edits"
	"rustwasm/server/imports"
	"rustwasm/server/store"
)

// The canonical bytes (RFC 8785) sort the keys, so the records sit
// between the two tags.
func isCurrentEdits(body string) bool {
	return strings.HasPrefix(body, `{"kind":"edit_records","records":[`) && strings.HasSuffix(strings.TrimSpace(body), `],"schema_version":2}`)
}

func TestAnOlderEditDocumentMigratesAtReceive(t *testing.T) {
	s := newStoreServer(t, store.NewMemory())
	ts := httptest.NewServer(storeMux(s))
	defer ts.Close()

	code, _, body := httpPost(t, ts.URL+"/edits", editRecords("e1", "accepted", 1, 1000))
	if code != 200 {
		t.Fatalf("an older client's document is accepted: %d %s", code, body)
	}
	if !isCurrentEdits(body) {
		t.Fatalf("the answer is the current version: %s", body)
	}
	data, head, err := s.edits().Now()
	if err != nil || head.Envelope.SchemaVersion != edits.Version {
		t.Fatalf("the head's envelope says v%d, want v%d (%v)", head.Envelope.SchemaVersion, edits.Version, err)
	}
	if !isCurrentEdits(string(data)) || !strings.Contains(string(data), `"id":"e1"`) {
		t.Fatalf("the head is the migrated document, its record kept: %s", data)
	}

	req := `{"from":"` + head.Hash + `","client":` + editRecords("e2", "accepted", 1, 2000) + `}`
	if code, _, body := httpPost(t, ts.URL+"/pointers/edits", req); code != 200 {
		t.Fatalf("POST /pointers/edits with a v1 client: %d %s", code, body)
	}
}

func TestANewerEditDocumentIsRefusedNamingBoth(t *testing.T) {
	s := newStoreServer(t, store.NewMemory())
	ts := httptest.NewServer(storeMux(s))
	defer ts.Close()
	newer := `{"kind":"edit_records","schema_version":3,"records":[]}`
	code, _, body := httpPost(t, ts.URL+"/edits", newer)
	if code != 400 || !strings.Contains(body, "edit_records v3") || !strings.Contains(body, "reads v2") {
		t.Fatalf("a newer document is refused naming both versions, never rewritten: %d %s", code, body)
	}
}

// The first entry is written by migrate:edits and names the old head's
// hash. The pointer and its blob stay as they were.
func TestAnOlderEditsPointerSeedsTheDisruptionsLog(t *testing.T) {
	bucket := store.NewMemory()
	old := []byte(editRecords("e1", "accepted", 3, 1000))
	st := store.New(bucket)
	oldHash, _, err := st.CAS("edits", "", old, store.Envelope{Kind: "edit_records", SchemaVersion: 1, ProducedBy: "cap-sync@sha256:old"}, "", "anonymous", time.Now().UnixMilli())
	if err != nil {
		t.Fatal(err)
	}
	st.Close()

	s := newStoreServer(t, bucket)
	entries, err := s.store.Entries(edits.DisruptionsLog, 0, 0)
	if err != nil || len(entries) != 1 {
		t.Fatalf("one entry: %d %v", len(entries), err)
	}
	var e struct {
		Kind   string `json:"kind"`
		Log    string `json:"log"`
		Seq    int64  `json:"seq"`
		Writer string `json:"writer"`
		Intent struct {
			Op       string `json:"op"`
			ArgsHash string `json:"args_hash"`
		} `json:"intent"`
		Changes []struct {
			Kind  string          `json:"kind"`
			ID    string          `json:"id"`
			After json.RawMessage `json:"after"`
		} `json:"changes"`
	}
	if err := json.Unmarshal([]byte(entries[0].Text), &e); err != nil {
		t.Fatal(err)
	}
	if e.Kind != "log_entry" || e.Log != "disruptions" || e.Seq != 1 || e.Writer != "migrate:edits" || e.Intent.Op != "migrate" || e.Intent.ArgsHash != oldHash {
		t.Fatalf("entry 1: %+v", e)
	}
	if len(e.Changes) != 2 || e.Changes[0].Kind != "edit_record" || e.Changes[0].ID != "e1" || e.Changes[1].Kind != "edit_records" {
		t.Fatalf("entry 1 copies the head's records, and the document's own record: %+v", e.Changes)
	}
	head, _, _ := s.store.Head("edits")
	if head.Hash != oldHash || head.Seq != 1 {
		t.Fatalf("the edits pointer stays read only: %+v", head)
	}
	if kept, _, err := s.store.GetBlob(oldHash); err != nil || string(kept) != string(old) {
		t.Fatalf("the older blob stays readable under its hash: %v", err)
	}
	ts := httptest.NewServer(storeMux(s))
	defer ts.Close()
	if code, _, body := httpGet(t, ts.URL+"/edits"); code != 200 || !strings.Contains(body, `"schema_version":2`) || !strings.Contains(body, `"id":"e1"`) {
		t.Fatalf("GET /edits serves the fold in the current version: %d %s", code, body)
	}

	newStoreServer(t, bucket)
	if again, _ := s.store.Entries(edits.DisruptionsLog, 0, 0); len(again) != 1 {
		t.Fatalf("a seeded log is not seeded again: %d entries", len(again))
	}
}

func v1Network(t *testing.T, id, effectiveFrom string) []byte {
	t.Helper()
	network := fixtureNetwork(t, id, id+" pick", effectiveFrom)
	delete(network, "kind")
	delete(network, "schema_version")
	body, err := json.Marshal(network)
	if err != nil {
		t.Fatal(err)
	}
	return body
}

func TestThePicksInUseCommitForwardAtStartup(t *testing.T) {
	bucket := store.NewMemory()
	s, _ := newPlansServer(t, bucket)

	picks := []struct{ id, from string }{{"summer", "2026-08-03"}, {"fall", "2026-09-07"}, {"winter", "2026-11-02"}}
	oldHashes := map[string]string{}
	var set json.RawMessage = json.RawMessage("null")
	for _, p := range picks {
		hash, _, err := s.store.CAS("plan."+p.id, "", v1Network(t, p.id, p.from), store.Envelope{Kind: "network_document", SchemaVersion: 1, ProducedBy: "cap-network@sha256:old"}, "", "anonymous", time.Now().UnixMilli())
		if err != nil {
			t.Fatal(err)
		}
		oldHashes[p.id] = hash
		var next struct {
			Document *string `json:"document"`
		}
		plan := map[string]any{"id": p.id, "name": p.id + " pick", "effective_from": p.from}
		if err := s.plans.Invoke(context.Background(), map[string]any{"op": "publish", "published": set, "plan": plan, "network_hash": hash, "today": "2026-08-01"}, &next); err != nil || next.Document == nil {
			t.Fatalf("publish %s: %v", p.id, err)
		}
		set = json.RawMessage(*next.Document)
	}
	oldSet, _, err := s.store.CAS("plans", "", set, s.planner().Envelope(), "", "anonymous", time.Now().UnixMilli())
	if err != nil {
		t.Fatal(err)
	}
	s.kept().Seed(nil)
	if got := plansHash(t, s); got != oldSet {
		t.Fatalf("the seeded log holds the pointer's set: %s, want %s", got, oldSet)
	}

	s.fixedToday = "2026-10-01"
	s.planner().CommitForward()

	if plansHash(t, s) == oldSet {
		t.Fatal("the set was republished with the migrated networks")
	}
	data := plansSet(t, s)
	var published publishedSet
	if err := json.Unmarshal(data, &published); err != nil {
		t.Fatal(err)
	}
	m, _ := s.migrator()
	for _, p := range published.Plans {
		switch p.ID {
		case "summer":
			if p.NetworkHash != oldHashes["summer"] {
				t.Errorf("a pick that stopped governing stays in its version: %s", p.NetworkHash)
			}
		case "fall", "winter":
			if p.NetworkHash == oldHashes[p.ID] {
				t.Errorf("pick %s in use came forward", p.ID)
				continue
			}
			body, _, err := s.store.GetBlob(p.NetworkHash)
			var tags struct {
				Kind          string `json:"kind"`
				SchemaVersion int    `json:"schema_version"`
			}
			if err != nil || json.Unmarshal(body, &tags) != nil || tags.Kind != "network" || tags.SchemaVersion != imports.NetworkVersion {
				t.Errorf("pick %s: the set names its network in the current version: %v %+v", p.ID, err, tags)
			}
			ptr, _, _ := s.store.Head("plan." + p.ID)
			if ptr.Hash != p.NetworkHash {
				t.Errorf("pick %s: plan.%s holds the network the set names", p.ID, p.ID)
			}
			entries, _, _ := s.store.Log("plan."+p.ID, "")
			if last := entries[len(entries)-1]; last.From != oldHashes[p.ID] || last.Writer != "migrate:"+m.Hash() {
				t.Errorf("pick %s: an ordinary logged CAS by the migration module: %+v", p.ID, last)
			}
		}
		if kept, _, err := s.store.GetBlob(oldHashes[p.ID]); err != nil || len(kept) == 0 {
			t.Errorf("pick %s: the older network stays readable under its hash: %v", p.ID, err)
		}
	}
	head, _ := s.store.LogHeadOf("plans")
	last, _ := s.store.Entries("plans", head.Seq-1, head.Seq)
	var entry struct {
		Writer string `json:"writer"`
	}
	if len(last) != 1 || json.Unmarshal([]byte(last[0].Text), &entry) != nil || entry.Writer != "migrate:"+m.Hash() {
		t.Fatalf("the plans log's last entry names the migration module: %+v", last)
	}
	// Each frozen version brought forward pins the version it came from
	// (D-232).
	_, frozenDoc, _, err := s.kept().Head("frozen_versions")
	var frozen struct {
		Versions map[string]struct {
			PlanID          string `json:"plan_id"`
			FromNetworkHash string `json:"from_network_hash"`
		} `json:"versions"`
	}
	if err != nil || json.Unmarshal(frozenDoc, &frozen) != nil {
		t.Fatalf("the frozen versions: %v", err)
	}
	for _, p := range published.Plans {
		if p.ID == "summer" {
			continue
		}
		if got := frozen.Versions[p.NetworkHash].FromNetworkHash; got != oldHashes[p.ID] {
			t.Errorf("pick %s: its frozen version pins its source %s, got %q", p.ID, oldHashes[p.ID], got)
		}
	}

	s.planner().CommitForward()
	if after, _ := s.store.LogHeadOf("plans"); after.Seq != head.Seq {
		t.Fatalf("current picks are not committed again: entry %d, want %d", after.Seq, head.Seq)
	}
}
