package main

// Document versions at the server's receive boundaries
// (docs/FORMATS.md, "Versions"):
//
//   - a POST in an older version (an edit document v1, the bare array
//     a client on an older build sends) comes forward through
//     cap-migrate before the merge; the head is always the current
//     version;
//   - a server started on a store holding an older head commits it
//     forward as an ordinary logged CAS (writer migrate:<module hash>),
//     and the older blob stays readable under its hash;
//   - a server started on a store whose picks in use (the pick in
//     force, and those still to come) hold an older network commits
//     each network forward and republishes the set through cap-plans;
//     a pick that stopped governing stays in its version.

import (
	"context"
	"encoding/json"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"rustwasm/server/store"
)

// isCurrentEdits reports whether body is an edit document in the
// current version, as its canonical bytes (RFC 8785, D-169) write it:
// keys sorted, so the records sit between the two tags.
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
	data, head, err := s.disruptionsNow()
	if err != nil || head.Envelope.SchemaVersion != editsVersion {
		t.Fatalf("the head's envelope says v%d, want v%d (%v)", head.Envelope.SchemaVersion, editsVersion, err)
	}
	if !isCurrentEdits(string(data)) || !strings.Contains(string(data), `"id":"e1"`) {
		t.Fatalf("the head is the migrated document, its record kept: %s", data)
	}

	// The same through the generic pointer surface.
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

// A store an older build wrote, with the edit document as the edits
// pointer in an older version: at startup the disruptions log gets its
// first entry from that head (brought forward through cap-migrate),
// written by migrate:edits and naming the head's hash. The pointer and
// its blob stay as they were.
func TestAnOlderEditsPointerSeedsTheDisruptionsLog(t *testing.T) {
	bucket := store.NewMemory()
	old := []byte(editRecords("e1", "accepted", 3, 1000))
	st := store.New(bucket)
	oldHash, _, err := st.CAS("edits", "", old, store.Envelope{Kind: "edit_records", SchemaVersion: 1, ProducedBy: "cap-sync@sha256:old"}, "", "anonymous", time.Now().UnixMilli())
	if err != nil {
		t.Fatal(err)
	}
	st.Close()

	s := newStoreServer(t, bucket) // seeds the log
	entries, err := s.store.Entries(disruptionsLog, 0, 0)
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
			ID    string          `json:"id"`
			After json.RawMessage `json:"after"`
		} `json:"changes"`
	}
	if err := json.Unmarshal([]byte(entries[0].Text), &e); err != nil {
		t.Fatal(err)
	}
	if e.Kind != "disruptions_entry" || e.Log != "disruptions" || e.Seq != 1 || e.Writer != "migrate:edits" || e.Intent.Op != "migrate" || e.Intent.ArgsHash != oldHash {
		t.Fatalf("entry 1: %+v", e)
	}
	if len(e.Changes) != 1 || e.Changes[0].ID != "e1" {
		t.Fatalf("entry 1 copies the head's records: %+v", e.Changes)
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

	// A second start writes nothing.
	newStoreServer(t, bucket)
	if again, _ := s.store.Entries(disruptionsLog, 0, 0); len(again) != 1 {
		t.Fatalf("a seeded log is not seeded again: %d entries", len(again))
	}
}

// v1Network is a pick's network as an older build wrote it: untagged.
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

	// A store an older build wrote: three picks, each an untagged v1
	// network under its plan.<id> pointer, and the set naming them.
	// The fixture runs through 2026-12-31.
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
	oldSet, _, err := s.store.CAS("plans", "", set, s.plansEnvelope(), "", "anonymous", time.Now().UnixMilli())
	if err != nil {
		t.Fatal(err)
	}

	// Fall is in force, winter is still to come, and summer stopped
	// governing when fall took effect.
	s.fixedToday = "2026-10-01"
	s.commitPicksForward()

	head, _, _ := s.store.Head("plans")
	if head.Hash == oldSet {
		t.Fatal("the set was republished with the migrated networks")
	}
	data, _, _ := s.store.GetBlob(head.Hash)
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
			if err != nil || json.Unmarshal(body, &tags) != nil || tags.Kind != "network" || tags.SchemaVersion != networkVersion {
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
	entries, _, _ := s.store.Log("plans", "")
	if last := entries[len(entries)-1]; last.Writer != "migrate:"+m.Hash() {
		t.Fatalf("the set's last commit names the migration module: %+v", last)
	}

	// Already current: a second start commits nothing.
	s.commitPicksForward()
	if after, _, _ := s.store.Log("plans", ""); len(after) != len(entries) {
		t.Fatalf("current picks are not committed again: %d entries, want %d", len(after), len(entries))
	}
}
