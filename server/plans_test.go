package main

// Publish → serve → resolve (docs/ROADMAP.md 1): a pick is published
// to the store — its network under the plan.<id> pointer, the
// published plan set under the plans pointer — the manifest pins the
// set as plans_hash, and a client resolves the pick in force through
// cap-plans and loads that network by hash. The modules own every
// byte and every answer: cap-network canonicalizes the network and
// validates it, cap-plans answers the next published set; the host
// stores exactly what they answer.

import (
	"bytes"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"strconv"
	"strings"
	"sync"
	"testing"
)

const plansWasm = "../web/pkg/cap_plans.wasm"

func newPlansHost(t *testing.T) *WasmHost {
	t.Helper()
	host, err := NewWasmHost(context.Background(), plansWasm)
	if err != nil {
		t.Fatalf("load plans module: %v", err)
	}
	t.Cleanup(func() { host.Close(context.Background()) })
	return host
}

// newPlansServer is a store-mode server that publishes picks.
func newPlansServer(t *testing.T, dir string) (*server, string) {
	t.Helper()
	s := newStoreServer(t, dir)
	s.plans = newPlansHost(t)
	s.networkWasm = networkWasm
	mux := storeMux(s)
	mux.HandleFunc("/plans", s.handlePlans)
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	return s, srv.URL
}

// fixtureImport is the e2e fixture feed imported through cap-network
// ONCE per test binary: each import instantiates the 3 MB network
// module, and a fresh instance per pick multiplied memory past what a
// developer VM survives.
var (
	fixtureOnce sync.Once
	fixtureDoc  []byte
	fixtureErr  error
)

// fixtureNetwork frames a copy of the imported fixture as a pick — the
// document a planner publishes.
func fixtureNetwork(t *testing.T, id, name, effectiveFrom string) map[string]any {
	t.Helper()
	fixtureOnce.Do(func() {
		zipBytes, err := os.ReadFile("../web/e2e/fixtures/sample-gtfs.zip")
		if err != nil {
			fixtureErr = err
			return
		}
		answer, _, err := importFeed(context.Background(), networkWasm, zipBytes)
		if err != nil {
			fixtureErr = err
			return
		}
		var parts struct {
			Network json.RawMessage `json:"network"`
		}
		fixtureErr = json.Unmarshal(answer, &parts)
		fixtureDoc = parts.Network
	})
	if fixtureErr != nil {
		t.Fatal(fixtureErr)
	}
	var network map[string]any
	if err := json.Unmarshal(fixtureDoc, &network); err != nil {
		t.Fatal(err)
	}
	plan := map[string]any{"id": id, "name": name}
	if effectiveFrom != "" {
		plan["effective_from"] = effectiveFrom
	}
	network["plan"] = plan
	return network
}

// publishPick posts a network the way any client may: an arbitrary
// serialization (pretty-printed here), never the canonical bytes.
func publishPick(t *testing.T, base string, network map[string]any) (int, map[string]any) {
	t.Helper()
	body, _ := json.MarshalIndent(network, "", "  ")
	code, _, resp := httpPost(t, base+"/plans", string(body))
	var out map[string]any
	json.Unmarshal([]byte(resp), &out)
	if out == nil {
		out = map[string]any{"raw": resp}
	}
	return code, out
}

type publishedSet struct {
	SchemaVersion int `json:"schema_version"`
	Plans         []struct {
		ID            string `json:"id"`
		Name          string `json:"name"`
		EffectiveFrom string `json:"effective_from"`
		NetworkHash   string `json:"network_hash"`
	} `json:"plans"`
}

func manifestPlansHash(t *testing.T, base string) string {
	t.Helper()
	_, _, body := httpGet(t, base+"/manifest")
	var m struct {
		PlansHash string `json:"plans_hash"`
	}
	json.Unmarshal([]byte(body), &m)
	return m.PlansHash
}

func sha(b []byte) string {
	sum := sha256.Sum256(b)
	return "sha256:" + hex.EncodeToString(sum[:])
}

// The headline pin: publish two picks, restart, and a client resolves
// the pick in force from the manifest alone — the published set by
// hash, the pick through cap-plans, its network by hash, every byte
// the hash names.
func TestPublishedPicksResolveThroughTheManifest(t *testing.T) {
	dir := t.TempDir()
	s, base := newPlansServer(t, dir)

	if h := manifestPlansHash(t, base); h != "" {
		t.Fatalf("nothing published yet, the manifest pins no plans: %q", h)
	}
	fall := fixtureNetwork(t, "fall-2026", "Fall 2026", "2026-09-07")
	code, out := publishPick(t, base, fall)
	if code != 200 {
		t.Fatalf("publish fall: %d %v", code, out)
	}
	spring := fixtureNetwork(t, "spring-2027", "Spring 2027", "2027-03-01")
	spring["name"] = "spring network" // a different document, a different hash
	if code, out = publishPick(t, base, spring); code != 200 {
		t.Fatalf("publish spring: %d %v", code, out)
	}
	plansHash, _ := out["plans_hash"].(string)
	springHash, _ := out["network_hash"].(string)
	if plansHash == "" || springHash == "" {
		t.Fatalf("publish answers the set and the network by hash: %v", out)
	}
	s.store.Close()

	// A restarted server over the same store serves the same set.
	_, base = newPlansServer(t, dir)
	if got := manifestPlansHash(t, base); got != plansHash {
		t.Fatalf("manifest plans_hash after restart = %q, want %q", got, plansHash)
	}
	code, hdr, body := httpGet(t, base+"/blobs/"+plansHash)
	if code != 200 || hdr.Get("X-Document-Kind") != "plans" || sha([]byte(body)) != plansHash {
		t.Fatalf("plans blob: %d kind=%q", code, hdr.Get("X-Document-Kind"))
	}
	var set publishedSet
	if err := json.Unmarshal([]byte(body), &set); err != nil || set.SchemaVersion != 1 || len(set.Plans) != 2 {
		t.Fatalf("published set: %v %+v", err, set)
	}
	if set.Plans[0].ID != "fall-2026" || set.Plans[1].ID != "spring-2027" {
		t.Fatalf("picks in date order: %+v", set.Plans)
	}

	// The client's question, through the same module the browser runs.
	host := newPlansHost(t)
	inForce := func(date string) (id, hash string) {
		var ans struct {
			Plan *struct {
				ID          string `json:"id"`
				NetworkHash string `json:"network_hash"`
			} `json:"plan"`
		}
		if err := host.Invoke(context.Background(), map[string]any{"op": "in_force", "published": json.RawMessage(body), "date": date}, &ans); err != nil {
			t.Fatal(err)
		}
		if ans.Plan == nil {
			return "", ""
		}
		return ans.Plan.ID, ans.Plan.NetworkHash
	}
	if id, _ := inForce("2026-08-01"); id != "" {
		t.Fatalf("before the first pick nothing is in force, got %q", id)
	}
	id, networkHash := inForce("2026-11-05")
	if id != "fall-2026" || networkHash != set.Plans[0].NetworkHash {
		t.Fatalf("in force 2026-11-05: %q %q", id, networkHash)
	}
	if id, h := inForce("2027-03-01"); id != "spring-2027" || h != springHash {
		t.Fatalf("in force on the spring pick date: %q %q", id, h)
	}

	// The pick's network, by hash: canonical bytes the hash names, and
	// gzipped for a page that registers it compressed.
	code, hdr, doc := httpGet(t, base+"/blobs/"+networkHash)
	if code != 200 || hdr.Get("X-Document-Kind") != "network_document" || sha([]byte(doc)) != networkHash {
		t.Fatalf("network blob: %d kind=%q", code, hdr.Get("X-Document-Kind"))
	}
	req, _ := http.NewRequest("GET", base+"/blobs/"+networkHash, nil)
	req.Header.Set("Accept", "application/gzip")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	gz, _ := io.ReadAll(resp.Body)
	resp.Body.Close()
	if resp.Header.Get("Content-Type") != "application/gzip" || resp.Header.Get("Content-Encoding") != "" {
		t.Fatalf("gzipped blob: type=%q encoding=%q", resp.Header.Get("Content-Type"), resp.Header.Get("Content-Encoding"))
	}
	zr, err := gzip.NewReader(bytes.NewReader(gz))
	if err != nil {
		t.Fatal(err)
	}
	plain, _ := io.ReadAll(zr)
	if sha(plain) != networkHash {
		t.Fatal("the gzipped blob decodes to the bytes the hash names")
	}
}

// What a pick must be: a valid, dated, versioned network that keeps the
// chain valid. A refusal says why and moves no pointer.
func TestPublishRefusesWhatWouldBreakTheChain(t *testing.T) {
	s, base := newPlansServer(t, t.TempDir())
	if code, out := publishPick(t, base, fixtureNetwork(t, "fall", "Fall", "2026-09-07")); code != 200 {
		t.Fatalf("publish: %d %v", code, out)
	}
	before := mustHead(t, s, "plans")

	cases := map[string]map[string]any{
		"an unversioned import": fixtureNetwork(t, "", "", ""),
		"an undated pick":       fixtureNetwork(t, "winter", "Winter", ""),
		"a second pick the same day": func() map[string]any {
			n := fixtureNetwork(t, "other", "Other", "2026-09-07")
			n["name"] = "other"
			return n
		}(),
		"an id that is not a pointer name": fixtureNetwork(t, "../escape", "Bad", "2027-01-01"),
	}
	for what, network := range cases {
		code, out := publishPick(t, base, network)
		if code != http.StatusUnprocessableEntity && code != http.StatusBadRequest {
			t.Fatalf("%s must be refused: %d %v", what, code, out)
		}
		if mustHead(t, s, "plans") != before {
			t.Fatalf("%s moved the plans pointer", what)
		}
	}
	// An invalid network (validation issues) never publishes.
	broken := fixtureNetwork(t, "broken", "Broken", "2027-02-01")
	for _, p := range broken["patterns"].(map[string]any) {
		p.(map[string]any)["route"] = "ghost" // UnknownRoute on every pattern
	}
	if code, out := publishPick(t, base, broken); code != http.StatusUnprocessableEntity {
		t.Fatalf("an invalid network must be refused with its issues: %d %v", code, out)
	} else if !strings.Contains(toJSON(out), "unknown_route") {
		t.Fatalf("the refusal names the issues: %v", out)
	}
	if _, found, _ := s.store.Head("plan.broken"); found {
		t.Fatal("a refused pick commits nothing")
	}

	// Republishing a pick moves it; the published networks stay
	// referenced (the plan.<id> log names every one), so GC keeps them.
	again := fixtureNetwork(t, "fall", "Fall", "2026-09-14")
	again["name"] = "fall, revised"
	if code, out := publishPick(t, base, again); code != 200 {
		t.Fatalf("republish: %d %v", code, out)
	}
	entries, _, _ := s.store.Log("plan.fall", "")
	if len(entries) != 2 {
		t.Fatalf("plan.fall log: %d entries", len(entries))
	}
	if _, err := s.store.GC(0); err != nil {
		t.Fatal(err)
	}
	for _, e := range entries {
		if _, _, err := s.store.GetBlob(e.To); err != nil {
			t.Fatalf("published network %s collected: %v", e.To, err)
		}
	}
}

func toJSON(v any) string {
	b, _ := json.Marshal(v)
	return string(b)
}

// The published set and the picks' networks are bus documents whose
// identity a module answers: the generic pointer surface never writes
// them, so nothing reaches a client that cap-network did not validate
// and cap-plans did not accept.
func TestPicksPublishOnlyThroughPlans(t *testing.T) {
	s, base := newPlansServer(t, t.TempDir())
	if code, out := publishPick(t, base, fixtureNetwork(t, "fall", "Fall", "2026-09-07")); code != 200 {
		t.Fatalf("publish: %d %v", code, out)
	}
	before := mustHead(t, s, "plans")
	forged := `{"schema_version":1,"plans":[{"id":"x","name":"x","effective_from":"2026-01-01","network_hash":"sha256:` + strings.Repeat("00", 32) + `"}]}`
	for _, req := range []struct{ pointer, body string }{
		{"plans", `{"from":"` + before + `","document":` + strconv.Quote(forged) + `}`},
		{"plan.fall", `{"from":"","document":"{}"}`},
		{"plan.new", `{"kind":"network_document","document":"{}"}`},
		{"elsewhere", `{"kind":"plans","document":` + strconv.Quote(forged) + `}`},
	} {
		code, _, body := httpPost(t, base+"/pointers/"+req.pointer, req.body)
		if code != http.StatusBadRequest {
			t.Fatalf("raw write to %s: %d %s", req.pointer, code, body)
		}
	}
	if mustHead(t, s, "plans") != before {
		t.Fatal("a raw write moved the published set")
	}
	if _, found, _ := s.store.Head("elsewhere"); found {
		t.Fatal("a plans document landed under another pointer")
	}
}
