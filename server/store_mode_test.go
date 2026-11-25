package main

// Store-mode round trips (docs/FORMATS.md): persistence through a
// restart, CAS conflict, idempotent redelivery, delta sync over the
// log, and the D-56 clock clamp.

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"testing"
	"time"

	"rustwasm/server/store"
)

// A second newStoreServer over the same backend is a restarted process.
func newStoreServer(t *testing.T, b store.Backend) *server {
	t.Helper()
	s := &server{wasm: newTestHost(t), retentionDays: 90}
	st := store.NewServing(b)
	s.store = st
	s.seedStore("")
	t.Cleanup(func() { st.Close() })
	return s
}

func storeMux(s *server) *http.ServeMux {
	mux := http.NewServeMux()
	mux.HandleFunc("/edits", s.handleEdits)
	mux.HandleFunc("/pointers/", s.handlePointers)
	mux.HandleFunc("/logs/", s.handleLogs)
	mux.HandleFunc("/blobs/", s.handleBlob)
	mux.HandleFunc("/manifest", s.handleManifest)
	return mux
}

func editDoc(id, status string, rev, at int64) string {
	return `{"kind":"edit_records","schema_version":2,"records":` + editRecords(id, status, rev, at) + `}`
}

// editRecords is the same record as a bare array, the edit document v1
// that a client on an older build posts.
func editRecords(id, status string, rev, at int64) string {
	diversion := `{"id":"d-` + id + `","route_id":"10","direction_id":null,` +
		`"first_kept_stop":"A","last_kept_stop":"B","skipped_stops":["C"],` +
		`"detour":null,"added_secs":300,"start_date":"` + date + `","end_date":"` + date + `"}`
	return `[{"id":"` + id + `","diversion":` + diversion +
		`,"status":"` + status + `","rev":` + strconv.FormatInt(rev, 10) +
		`,"updated_at":` + strconv.FormatInt(at, 10) + `}]`
}

func httpPost(t *testing.T, url, body string) (int, http.Header, string) {
	t.Helper()
	resp, err := http.Post(url, "application/json", strings.NewReader(body))
	if err != nil {
		t.Fatalf("post %s: %v", url, err)
	}
	defer resp.Body.Close()
	b, _ := io.ReadAll(resp.Body)
	return resp.StatusCode, resp.Header, string(b)
}

func httpGet(t *testing.T, url string) (int, http.Header, string) {
	t.Helper()
	resp, err := http.Get(url)
	if err != nil {
		t.Fatalf("get %s: %v", url, err)
	}
	defer resp.Body.Close()
	b, _ := io.ReadAll(resp.Body)
	return resp.StatusCode, resp.Header, string(b)
}

// An edit document, tombstone included, survives a restart.
func TestStoreEditsSurviveRestart(t *testing.T) {
	bucket := store.NewMemory()
	s1 := newStoreServer(t, bucket)
	base := httptest.NewServer(storeMux(s1)).URL

	code, hdr, body := httpPost(t, base+"/edits", editDoc("e1", "draft", 1, 100))
	if code != 200 || !strings.Contains(body, `"status":"submitted"`) {
		t.Fatalf("post draft: %d %s", code, body)
	}
	etag := strings.Trim(hdr.Get("ETag"), `"`)

	// The retire bumps the rev past the submitted rev 2, because a
	// same-millisecond stamp ties on updated_at.
	code, _, body = httpPost(t, base+"/edits", editDoc("e1", "retired", 3, time.Now().UnixMilli()))
	if code != 200 || !strings.Contains(body, `"status":"retired"`) {
		t.Fatalf("post tombstone: %d %s", code, body)
	}

	s1.store.Close()
	s2 := newStoreServer(t, bucket)
	base2 := httptest.NewServer(storeMux(s2)).URL

	code, hdr2, body := httpGet(t, base2+"/edits")
	if code != 200 || !strings.Contains(body, `"retired"`) {
		t.Fatalf("after restart: %d %s", code, body)
	}
	if hdr2.Get("ETag") == `""`+etag+`""` {
		t.Fatal("etag must move on a new commit")
	}

	log := logEntries(t, base2, 0)
	if log.Head != 2 || len(log.Entries) != 2 || log.Entries[0].Seq != 1 || log.Entries[1].Seq != 2 {
		t.Fatalf("the log: %+v", log)
	}
	second := log.Entries[1].Entry
	if second.Kind != "log_entry" || !strings.HasPrefix(second.Module, "cap-sync@sha256:") || second.Intent.Op != "POST /edits" ||
		len(second.Changes) != 1 || second.Changes[0].ID != "e1" || !strings.HasPrefix(second.Changes[0].Before, "sha256:") {
		t.Fatalf("entry 2: %+v", second)
	}
}

type loggedEntry struct {
	Kind    string `json:"kind"`
	Seq     int64  `json:"seq"`
	Writer  string `json:"writer"`
	Module  string `json:"module"`
	Reverts int64  `json:"reverts"`
	Intent  struct {
		Op   string `json:"op"`
		Note string `json:"note"`
	} `json:"intent"`
	Changes []struct {
		ID     string          `json:"id"`
		Before string          `json:"before"`
		After  json.RawMessage `json:"after"`
	} `json:"changes"`
}

type logAnswer struct {
	Head        int64   `json:"head"`
	Gaps        []int64 `json:"gaps"`
	Checkpoint  int64   `json:"checkpoint"`
	Document    string  `json:"document"`
	Quarantined []struct {
		Seq    int64  `json:"seq"`
		Reason string `json:"reason"`
	} `json:"quarantined"`
	Entries []struct {
		Seq        int64       `json:"seq"`
		Entry      loggedEntry `json:"entry"`
		Unreadable string      `json:"unreadable"`
	} `json:"entries"`
}

func logEntries(t *testing.T, base string, since int64) logAnswer {
	t.Helper()
	code, _, body := httpGet(t, base+"/logs/disruptions?since="+strconv.FormatInt(since, 10))
	var a logAnswer
	if code != 200 || json.Unmarshal([]byte(body), &a) != nil {
		t.Fatalf("GET /logs/disruptions?since=%d: %d %s", since, code, body)
	}
	return a
}

// A stale from answers 409 with the current hash. Re-posting the same
// document converges to the same head hash.
func TestStoreCASConflictAndRedelivery(t *testing.T) {
	s := newStoreServer(t, store.NewMemory())
	base := httptest.NewServer(storeMux(s)).URL

	code, _, body := httpPost(t, base+"/edits", editDoc("e1", "draft", 1, 100))
	if code != 200 {
		t.Fatalf("post: %d %s", code, body)
	}
	_, _, headBody := httpGet(t, base+"/pointers/edits")
	var head struct {
		Hash string `json:"hash"`
	}
	if err := json.Unmarshal([]byte(headBody), &head); err != nil {
		t.Fatal(err)
	}

	req, _ := json.Marshal(map[string]any{"from": "", "client": json.RawMessage(editDoc("e2", "draft", 1, 100))})
	resp, err := http.Post(base+"/pointers/edits", "application/json", strings.NewReader(string(req)))
	if err != nil {
		t.Fatal(err)
	}
	if resp.StatusCode != http.StatusConflict {
		b, _ := io.ReadAll(resp.Body)
		resp.Body.Close()
		t.Fatalf("stale from must 409, got %d: %s", resp.StatusCode, b)
	}
	var ce struct {
		Current string `json:"current"`
	}
	json.NewDecoder(resp.Body).Decode(&ce)
	resp.Body.Close()
	if ce.Current != head.Hash {
		t.Fatalf("409 must carry the current hash: %q vs %q", ce.Current, head.Hash)
	}

	// The head may move once for the new record. Posting the identical
	// document again must not change the head hash.
	_, _, b1 := httpPost(t, base+"/edits", editDoc("e2", "draft", 1, 100))
	_, _, head1 := httpGet(t, base+"/pointers/edits")
	_, _, b2 := httpPost(t, base+"/edits", editDoc("e2", "draft", 1, 100))
	_, _, head2 := httpGet(t, base+"/pointers/edits")
	_ = b1
	_ = b2
	if head1 != head2 { // the same hash and the same seq
		t.Fatalf("redelivering the same document changed the head: %s -> %s", head1, head2)
	}
}

// A reader holding entry N gets only the later entries. GET /edits
// revalidates for 304 on the fold's hash.
func TestStoreDeltaSyncAndRevalidation(t *testing.T) {
	s := newStoreServer(t, store.NewMemory())
	base := httptest.NewServer(storeMux(s)).URL

	httpPost(t, base+"/edits", editDoc("e1", "draft", 1, 100))
	httpPost(t, base+"/edits", editDoc("e2", "draft", 1, 100))
	httpPost(t, base+"/edits", editDoc("e3", "draft", 1, 100))

	full := logEntries(t, base, 0)
	if full.Head != 3 || len(full.Entries) != 3 {
		t.Fatalf("entries: %+v", full)
	}
	delta := logEntries(t, base, 1)
	if len(delta.Entries) != 2 || delta.Entries[0].Seq != 2 || delta.Entries[0].Entry.Changes[0].ID != "e2" {
		t.Fatalf("delta: %+v", delta)
	}
	if none := logEntries(t, base, 3); len(none.Entries) != 0 {
		t.Fatalf("nothing after the head: %+v", none)
	}
	if code, _, _ := httpGet(t, base+"/logs/disruptions?since=x"); code != 400 {
		t.Fatalf("a since that is no seq: %d", code)
	}

	code, hdr, body := httpGet(t, base+"/edits")
	if code != 200 || hdr.Get("ETag") != `"`+full.Document+`"` || !strings.Contains(body, `"id":"e3"`) {
		t.Fatalf("the fold: %d %v %s", code, hdr, body)
	}
	req, _ := http.NewRequest("GET", base+"/edits", nil)
	req.Header.Set("If-None-Match", hdr.Get("ETag"))
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusNotModified {
		t.Fatalf("etag revalidation: %d", resp.StatusCode)
	}
}

// A revert puts e1 back in force as a new entry. A page that resends
// the retired copy does not undo it. A second revert of entry 2 is
// refused (409), because entry 3 changed the record since.
func TestStoreRevertIsANewEntry(t *testing.T) {
	s := newStoreServer(t, store.NewMemory())
	base := httptest.NewServer(storeMux(s)).URL

	httpPost(t, base+"/edits", editDoc("e1", "accepted", 1, 100))
	retired := editDoc("e1", "retired", 2, time.Now().UnixMilli())
	if code, _, body := httpPost(t, base+"/edits", retired); code != 200 || !strings.Contains(body, `"retired"`) {
		t.Fatalf("retire: %d %s", code, body)
	}
	code, _, body := httpPost(t, base+"/logs/disruptions/revert", `{"seq":2}`)
	if code != 200 || !strings.Contains(body, `"seq":3`) || !strings.Contains(body, `"reverts":2`) {
		t.Fatalf("revert: %d %s", code, body)
	}
	_, _, doc := httpGet(t, base+"/edits")
	if !strings.Contains(doc, `"status":"accepted"`) {
		t.Fatalf("e1 is in force again: %s", doc)
	}
	if third := logEntries(t, base, 2).Entries[0].Entry; third.Reverts != 2 || third.Intent.Op != "POST /logs/disruptions/revert" {
		t.Fatalf("entry 3: %+v", third)
	}
	if _, _, again := httpPost(t, base+"/edits", retired); !strings.Contains(again, `"status":"accepted"`) {
		t.Fatalf("a page's stale retired copy undid the revert: %s", again)
	}
	if code, _, body := httpPost(t, base+"/logs/disruptions/revert", `{"seq":2}`); code != http.StatusConflict || !strings.Contains(body, "record e1") {
		t.Fatalf("a second revert: %d %s", code, body)
	}
	if code, _, _ := httpPost(t, base+"/logs/disruptions/revert", `{"seq":9}`); code != http.StatusNotFound {
		t.Fatalf("a revert past the head: %d", code)
	}
}

// The fold skips an entry that does not read and names it. A gap stops
// writes (503) until an entry that names the cause closes it.
func TestStoreQuarantineAndGap(t *testing.T) {
	bucket := store.NewMemory()
	s := newStoreServer(t, bucket)
	base := httptest.NewServer(storeMux(s)).URL
	httpPost(t, base+"/edits", editDoc("e1", "accepted", 1, 100))
	if err := s.store.AppendEntry(disruptionsLog, 2, []byte(`{"kind":"log_entry"`)); err != nil {
		t.Fatal(err)
	}
	if code, _, body := httpPost(t, base+"/edits", editDoc("e2", "accepted", 1, 100)); code != 200 || !strings.Contains(body, `"e1"`) || !strings.Contains(body, `"e2"`) {
		t.Fatalf("the fold goes on past a quarantined entry: %d %s", code, body)
	}
	if a := logEntries(t, base, 0); len(a.Quarantined) != 1 || a.Quarantined[0].Seq != 2 || a.Entries[1].Unreadable == "" || a.Head != 3 {
		t.Fatalf("entry 2 is named: %+v", a)
	}

	bucket.Delete(context.Background(), "logs/disruptions/entries/00000000000000000002.json")
	s2 := newStoreServer(t, bucket)
	base2 := httptest.NewServer(storeMux(s2)).URL
	if code, hdr, body := httpPost(t, base2+"/edits", editDoc("e4", "accepted", 1, 100)); code != http.StatusServiceUnavailable || hdr.Get("Retry-After") == "" || !strings.Contains(body, "gap") {
		t.Fatalf("a write over a gap: %d %s", code, body)
	}
	if code, _, _ := httpPost(t, base2+"/logs/disruptions/close-gap", `{"seq":2}`); code != 400 {
		t.Fatalf("a close without a cause: %d", code)
	}
	if code, _, body := httpPost(t, base2+"/logs/disruptions/close-gap", `{"seq":4,"cause":"x"}`); code != http.StatusConflict {
		t.Fatalf("a close where there is no gap: %d %s", code, body)
	}
	if code, _, body := httpPost(t, base2+"/logs/disruptions/close-gap", `{"seq":2,"cause":"deleted by hand in the console"}`); code != 200 {
		t.Fatalf("close the gap: %d %s", code, body)
	}
	if code, _, body := httpPost(t, base2+"/edits", editDoc("e4", "accepted", 1, 100)); code != 200 || !strings.Contains(body, `"e2"`) || !strings.Contains(body, `"e4"`) {
		t.Fatalf("writes go on: %d %s", code, body)
	}
	if closer := logEntries(t, base2, 1).Entries[0].Entry; closer.Intent.Note != "deleted by hand in the console" || len(closer.Changes) != 0 {
		t.Fatalf("entry 3 names the cause: %+v", closer)
	}
}

// A fresh process boots from a checkpoint and serves the same document.
func TestStoreBootsFromACheckpoint(t *testing.T) {
	bucket := store.NewMemory()
	s := newStoreServer(t, bucket)
	base := httptest.NewServer(storeMux(s)).URL
	for i := 0; i < checkpointEvery+3; i++ {
		id := "e" + strconv.Itoa(i)
		if code, _, body := httpPost(t, base+"/edits", editDoc(id, "accepted", 1, 100)); code != 200 {
			t.Fatalf("post %d: %d %s", i, code, body)
		}
	}
	_, _, before := httpGet(t, base+"/edits")
	if cps, _ := s.store.Checkpoints(disruptionsLog); len(cps) != 1 || cps[0] != checkpointEvery {
		t.Fatalf("checkpoints: %v", cps)
	}
	s2 := newStoreServer(t, bucket)
	base2 := httptest.NewServer(storeMux(s2)).URL
	if _, _, after := httpGet(t, base2+"/edits"); after != before {
		t.Fatal("a process booted from the checkpoint serves another document")
	}
	if s2.keptLog(disruptionsLog).fold.stored != checkpointEvery {
		t.Fatalf("the boot started from checkpoint %d", s2.keptLog(disruptionsLog).fold.stored)
	}
}

// A client whose clock runs 10 h fast cannot create a record that wins past now
// (D-56). The stored stamp is clamped to the server clock.
func TestStoreReceiveClampsTheClock(t *testing.T) {
	s := newStoreServer(t, store.NewMemory())
	base := httptest.NewServer(storeMux(s)).URL

	before := time.Now().UnixMilli()
	future := time.Now().Add(10 * time.Hour).UnixMilli()
	code, _, body := httpPost(t, base+"/edits", editDoc("e1", "accepted", 9, future))
	if code != 200 {
		t.Fatalf("post: %d %s", code, body)
	}
	var tagged struct {
		Records []struct {
			ID        string `json:"id"`
			UpdatedAt int64  `json:"updated_at"`
		} `json:"records"`
	}
	if err := json.Unmarshal([]byte(body), &tagged); err != nil || len(tagged.Records) != 1 {
		t.Fatalf("doc: %v %s", err, body)
	}
	doc := tagged.Records
	after := time.Now().UnixMilli()
	if doc[0].UpdatedAt < before || doc[0].UpdatedAt > after {
		t.Fatalf("stored updated_at %d outside [%d, %d]: the future stamp was not clamped", doc[0].UpdatedAt, before, after)
	}
}

// GET /manifest commits the manifest once. A second GET revalidates for
// 304 and appends no log entry.
func TestStoreManifestPointer(t *testing.T) {
	s := newStoreServer(t, store.NewMemory())
	base := httptest.NewServer(storeMux(s)).URL

	code, hdr, body := httpGet(t, base+"/manifest")
	if code != 200 || !strings.Contains(body, `"wasm_hash"`) {
		t.Fatalf("manifest: %d %s", code, body)
	}
	etag := hdr.Get("ETag")
	if etag == "" {
		t.Fatal("manifest must carry an ETag in store mode")
	}

	req, _ := http.NewRequest("GET", base+"/manifest", nil)
	req.Header.Set("If-None-Match", etag)
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusNotModified {
		t.Fatalf("manifest 304: %d", resp.StatusCode)
	}

	_, _, logBody := httpGet(t, base+"/pointers/manifest/log")
	var logResp struct {
		Entries []store.LogEntry `json:"entries"`
	}
	json.Unmarshal([]byte(logBody), &logResp)
	if len(logResp.Entries) != 1 || logResp.Entries[0].Kind != "manifest" || logResp.Entries[0].ProducedBy != "server" {
		t.Fatalf("manifest pointer: %+v", logResp.Entries)
	}
	if `"`+logResp.Entries[0].To+`"` != etag {
		t.Fatalf("manifest ETag %s != pointer %s", etag, logResp.Entries[0].To)
	}
}

// A host-owned kind such as policy posts verbatim with a from-CAS, and
// a wrong document_hash is rejected.
func TestStoreRawPointerCommit(t *testing.T) {
	s := newStoreServer(t, store.NewMemory())
	base := httptest.NewServer(storeMux(s)).URL

	mk := func(from, doc, hash string) (int, string) {
		req, _ := json.Marshal(map[string]any{"from": from, "kind": "policy", "document": doc, "document_hash": hash})
		code, _, body := httpPost(t, base+"/pointers/policy", string(req))
		return code, body
	}
	code, body := mk("", `{"schema_version":1,"rules":[]}`, "")
	if code != 200 {
		t.Fatalf("create policy: %d %s", code, body)
	}
	code, body = mk("", `{}`, "sha256:"+strings.Repeat("ff", 32))
	if code == 200 {
		t.Fatalf("wrong document_hash must fail: %s", body)
	}
	_, _, headBody := httpGet(t, base+"/pointers/policy")
	var head struct {
		Hash string `json:"hash"`
	}
	json.Unmarshal([]byte(headBody), &head)
	code, body = mk(head.Hash, `{"schema_version":1,"rules":[{"name":"r","roles":["a"],"pointers":["edits"],"actions":["read"]}]}`, "")
	if code != 200 {
		t.Fatalf("move policy: %d %s", code, body)
	}
	_, _, head2 := httpGet(t, base+"/pointers/policy")
	var head3 struct {
		Hash string `json:"hash"`
		Seq  int64  `json:"seq"`
	}
	json.Unmarshal([]byte(head2), &head3)
	if head3.Seq != 2 || head3.Hash == head.Hash {
		t.Fatalf("policy pointer: %+v", head3)
	}
}
