package main

// Pins that no server copy is a point of failure (docs/architecture/
// server.md). A copy has its own store handle over one shared backend,
// so it shares only the bucket with another copy, as two processes do.

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"rustwasm/server/store"
)

type faultBucket struct {
	store.Backend
	down atomic.Bool
	// delay is nanoseconds that each call waits, unless its context ends.
	delay atomic.Int64
	// stopped means nothing more from the copy reaches the bucket.
	stopped atomic.Bool

	mu     sync.Mutex
	faults []createFault
}

type createFault struct {
	prefix string
	kind   faultKind
}

type faultKind int

const (
	loseAnswer faultKind = iota + 1
	stopBefore
	stopAfter
)

var errAnswerLost = errors.New("read tcp: i/o timeout")

func (f *faultBucket) inject(prefix string, kind faultKind) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.faults = append(f.faults, createFault{prefix, kind})
}

func (f *faultBucket) wait(ctx context.Context) error {
	if f.down.Load() || f.stopped.Load() {
		return errBucketDown
	}
	if d := time.Duration(f.delay.Load()); d > 0 {
		select {
		case <-time.After(d):
		case <-ctx.Done():
			return ctx.Err()
		}
	}
	return nil
}

func (f *faultBucket) Get(ctx context.Context, k string) ([]byte, map[string]string, error) {
	if err := f.wait(ctx); err != nil {
		return nil, nil, err
	}
	return f.Backend.Get(ctx, k)
}
func (f *faultBucket) Stat(ctx context.Context, k string) (store.ObjectInfo, error) {
	if err := f.wait(ctx); err != nil {
		return store.ObjectInfo{}, err
	}
	return f.Backend.Stat(ctx, k)
}
func (f *faultBucket) List(ctx context.Context, p, a string) ([]store.ObjectInfo, error) {
	if err := f.wait(ctx); err != nil {
		return nil, err
	}
	return f.Backend.List(ctx, p, a)
}
func (f *faultBucket) Create(ctx context.Context, k string, d []byte, m map[string]string) error {
	if err := f.wait(ctx); err != nil {
		return err
	}
	f.mu.Lock()
	kind := faultKind(0)
	for i, c := range f.faults {
		if strings.HasPrefix(k, c.prefix) {
			kind = c.kind
			f.faults = append(f.faults[:i], f.faults[i+1:]...)
			break
		}
	}
	f.mu.Unlock()
	switch kind {
	case stopBefore:
		f.stopped.Store(true)
		return errBucketDown
	case stopAfter:
		err := f.Backend.Create(ctx, k, d, m)
		f.stopped.Store(true)
		if err != nil {
			return err
		}
		return errBucketDown
	case loseAnswer:
		if err := f.Backend.Create(ctx, k, d, m); err != nil {
			return err
		}
		return errAnswerLost
	}
	return f.Backend.Create(ctx, k, d, m)
}

// copyOver sets dir as the copy's disk cache when it is not empty.
func copyOver(t *testing.T, shared store.Backend, dir string) (*server, *faultBucket, string) {
	t.Helper()
	view := &faultBucket{Backend: shared}
	s := &server{wasm: newTestHost(t), retentionDays: 90}
	st := store.NewServing(view)
	if dir != "" {
		if err := st.KeepCache(dir, "mem://shared"); err != nil {
			t.Fatal(err)
		}
	}
	s.store = st
	s.seedStore("")
	closeModules(t, s)
	srv := httptest.NewServer(storeMux(s))
	t.Cleanup(srv.Close)
	return s, view, srv.URL
}

func changedIDs(t *testing.T, shared store.Backend, log string) map[int64][]string {
	t.Helper()
	entries, err := store.New(shared).Entries(log, 0, 0)
	if err != nil {
		t.Fatal(err)
	}
	out := map[int64][]string{}
	for i, e := range entries {
		if e.Seq != int64(i+1) {
			t.Fatalf("the %s log is not one order: entry %d at position %d", log, e.Seq, i+1)
		}
		var entry struct {
			Changes []struct {
				ID string `json:"id"`
			} `json:"changes"`
		}
		if err := json.Unmarshal([]byte(e.Text), &entry); err != nil {
			t.Fatalf("entry %d: %v", e.Seq, err)
		}
		for _, c := range entry.Changes {
			out[e.Seq] = append(out[e.Seq], c.ID)
		}
	}
	return out
}

// onceEach does not count the document's own record (id "document").
func onceEach(t *testing.T, log string, byEntry map[int64][]string, ids []string) {
	t.Helper()
	count := map[string]int{}
	for _, changed := range byEntry {
		for _, id := range changed {
			if id != "document" {
				count[id]++
			}
		}
	}
	for _, id := range ids {
		if count[id] != 1 {
			t.Errorf("the %s log: %s is written %d times, not once", log, id, count[id])
		}
	}
	if len(count) != len(ids) {
		t.Errorf("the %s log changes %d records, not %d: %v", log, len(count), len(ids), count)
	}
}

func noteID(copy, i int) string { return fmt.Sprintf("n-%d-%d", copy, i) }
func editID(copy, i int) string { return fmt.Sprintf("e-%d-%d", copy, i) }

func TestTwoCopiesOnOneStoreWriteTheSameLogAtTheSameTime(t *testing.T) {
	shared := store.NewMemory()
	_, viewA, baseA := copyOver(t, shared, "")
	_, viewB, baseB := copyOver(t, shared, "")
	// A slow bucket widens each race.
	viewA.delay.Store(int64(time.Millisecond))
	viewB.delay.Store(int64(time.Millisecond))

	const writers = 4
	var wg sync.WaitGroup
	var edits, notes []string
	for c, base := range []string{baseA, baseB} {
		for i := 0; i < writers; i++ {
			edits, notes = append(edits, editID(c, i)), append(notes, noteID(c, i))
			wg.Add(2)
			go func() {
				defer wg.Done()
				if code, _, body := httpPost(t, base+"/edits", editDoc(editID(c, i), "draft", 1, 100)); code != 200 {
					t.Errorf("copy %d edit %d: %d %s", c, i, code, body)
				}
			}()
			go func() {
				defer wg.Done()
				body := map[string]any{"key": fmt.Sprintf("copy-%d/%d", c, i), "ops": []any{writeNote(noteID(c, i), "text")}}
				if code, out := intentPost(t, base, "notes", body); code != 200 {
					t.Errorf("copy %d note %d: %d %v", c, i, code, out)
				}
			}()
		}
	}
	wg.Wait()
	if t.Failed() {
		return
	}
	onceEach(t, disruptionsLog, changedIDs(t, shared, disruptionsLog), edits)
	onceEach(t, "notes", changedIDs(t, shared, "notes"), notes)

	time.Sleep(store.FreshnessWindow + 100*time.Millisecond) // each copy probes the bucket again
	for _, path := range []string{"/edits", "/logs/disruptions", "/logs/notes"} {
		codeA, _, a := httpGet(t, baseA+path)
		codeB, _, b := httpGet(t, baseB+path)
		if codeA != 200 || codeB != 200 || a != b {
			t.Errorf("%s: both copies serve the same heads:\nA %d %s\nB %d %s", path, codeA, a, codeB, b)
		}
	}
}

func TestACopyStartedWhileTheBucketIsDownServesItsCache(t *testing.T) {
	shared := store.NewMemory()
	dir := t.TempDir()
	_, _, base := copyOver(t, shared, dir)
	if code, _, body := httpPost(t, base+"/edits", editDoc("e1", "draft", 1, 100)); code != 200 {
		t.Fatalf("post: %d %s", code, body)
	}
	if code, out := intentPost(t, base, "notes", map[string]any{"key": "desk/1", "ops": []any{writeNote("n1", "No operator.")}}); code != 200 {
		t.Fatalf("note: %d %v", code, out)
	}
	before := map[string]string{}
	for _, path := range []string{"/edits", "/manifest", "/logs/disruptions", "/logs/notes"} {
		code, hdr, body := httpGet(t, base+path)
		if code != 200 || hdr.Get("X-Store-Stale") != "" {
			t.Fatalf("%s while the bucket answers: %d %v", path, code, hdr)
		}
		before[path] = body
	}

	view := &faultBucket{Backend: shared}
	view.down.Store(true)
	again, _, base2 := copyOver(t, view, dir)
	for path, want := range before {
		code, hdr, body := httpGet(t, base2+path)
		if code != 200 || body != want || hdr.Get("X-Store-Stale") != "true" || hdr.Get("X-Store-As-Of") == "" {
			t.Errorf("%s from the cache, labelled stale: %d %v\n%s\nwant %s", path, code, hdr, body, want)
		}
	}
	if code, rd := readyz(t, again); code != 200 || rd.Status != "degraded" || rd.Store != "stale" || rd.StoreAsOfMs <= 0 {
		t.Errorf("readyz on the cache: %d %+v", code, rd)
	}
	held := editDoc("e2", "draft", 1, 200)
	if code, hdr, body := httpPost(t, base2+"/edits", held); code != http.StatusServiceUnavailable || hdr.Get("Retry-After") == "" {
		t.Errorf("an edit on the cache: %d %v %s", code, hdr, body)
	}
	note := map[string]any{"key": "desk/2", "ops": []any{writeNote("n2", "Covered.")}}
	if code, out := intentPost(t, base2, "notes", note); code != http.StatusServiceUnavailable {
		t.Errorf("a note on the cache: %d %v", code, out)
	}
	if t.Failed() {
		return
	}

	view.down.Store(false)
	if code, _, body := httpPost(t, base2+"/edits", held); code != 200 || !strings.Contains(body, `"e1"`) || !strings.Contains(body, `"e2"`) {
		t.Fatalf("the held edit lands once the bucket answers: %d %s", code, body)
	}
	if code, out := intentPost(t, base2, "notes", note); code != 200 {
		t.Fatalf("the held note lands: %d %v", code, out)
	}
	if _, hdr, _ := httpGet(t, base2+"/edits"); hdr.Get("X-Store-Stale") != "" {
		t.Fatalf("back up, reads are unlabelled: %v", hdr)
	}
	if code, rd := readyz(t, again); code != 200 || rd.Status != "ready" {
		t.Fatalf("back up, ready: %d %+v", code, rd)
	}
	onceEach(t, disruptionsLog, changedIDs(t, shared, disruptionsLog), []string{"e1", "e2"})
	onceEach(t, "notes", changedIDs(t, shared, "notes"), []string{"n1", "n2"})
}

func TestACopyWithNoCacheStartedWhileTheBucketIsDownIsUnavailable(t *testing.T) {
	shared := store.NewMemory()
	_, _, base := copyOver(t, shared, "")
	httpPost(t, base+"/edits", editDoc("e1", "draft", 1, 100))
	down := &faultBucket{Backend: shared}
	down.down.Store(true)
	s, _, base2 := copyOver(t, down, "")
	if code, _, _ := httpGet(t, base2+"/edits"); code != http.StatusServiceUnavailable {
		t.Errorf("no cache, nothing to serve: %d", code)
	}
	if code, rd := readyz(t, s); code != 503 || rd.Status != "unavailable" {
		t.Errorf("readyz with no cache: %d %+v", code, rd)
	}
}

// Pins that a write whose answer is lost lands once when the page
// resends.
func TestAWriteWhoseAnswerIsLostLandsOnceWhenResent(t *testing.T) {
	shared := store.NewMemory()
	s, view, base := copyOver(t, shared, "")

	view.inject("logs/disruptions/entries/", loseAnswer)
	doc := editDoc("e1", "draft", 1, 100)
	if code, hdr, body := httpPost(t, base+"/edits", doc); code != http.StatusServiceUnavailable || hdr.Get("Retry-After") == "" {
		t.Fatalf("a lost answer is no answer: %d %s", code, body)
	}
	if code, _, body := httpPost(t, base+"/edits", doc); code != 200 || !strings.Contains(body, `"e1"`) {
		t.Fatalf("the resend: %d %s", code, body)
	}

	view.inject("logs/notes/entries/", loseAnswer)
	note := map[string]any{"key": "tablet/1", "ops": []any{writeNote("n1", "No operator.")}}
	if code, out := intentPost(t, base, "notes", note); code != http.StatusServiceUnavailable {
		t.Fatalf("a lost answer is no answer: %d %v", code, out)
	}
	if code, out := intentPost(t, base, "notes", note); code != 200 || out["landed"] != true || out["seq"] != float64(1) {
		t.Fatalf("the resend answers the entry it landed in: %d %v", code, out)
	}

	// A pointer commit whose answer is lost converges on the same head.
	view.inject("pointers/policy/", loseAnswer)
	env := store.Envelope{Kind: "policy", SchemaVersion: 1, ProducedBy: "server"}
	if _, _, err := s.store.CAS("policy", "", []byte(`{"p":1}`), env, "", "w", 1); !errors.Is(err, store.ErrUnavailable) {
		t.Fatalf("a lost answer is unavailable: %v", err)
	}
	if hash, seq, err := s.store.CAS("policy", "", []byte(`{"p":1}`), env, "", "w", 2); err != nil || seq != 1 || hash != store.HashOf([]byte(`{"p":1}`)) {
		t.Fatalf("the resend is the kept record: %d %v", seq, err)
	}

	onceEach(t, disruptionsLog, changedIDs(t, shared, disruptionsLog), []string{"e1"})
	onceEach(t, "notes", changedIDs(t, shared, "notes"), []string{"n1"})
	if log, _, _ := store.New(shared).Log("policy", ""); len(log) != 1 {
		t.Fatalf("one policy record: %d", len(log))
	}
}

// Pins that a write interrupted by a stopped copy lands once when the
// page sends it to another copy.
func TestACopyStoppedInTheMiddleOfAWriteLosesNothingAndWritesNothingTwice(t *testing.T) {
	shared := store.NewMemory()
	_, a, baseA := copyOver(t, shared, "")
	_, _, baseB := copyOver(t, shared, "")

	a.inject("logs/disruptions/entries/", stopBefore)
	before := editDoc("e1", "draft", 1, 100)
	if code, _, _ := httpPost(t, baseA+"/edits", before); code == 200 {
		t.Fatal("a copy stopped before the create acknowledged the write")
	}
	if code, _, body := httpPost(t, baseB+"/edits", before); code != 200 || !strings.Contains(body, `"e1"`) {
		t.Fatalf("sent again to the other copy: %d %s", code, body)
	}

	_, a2, baseA2 := copyOver(t, shared, "") // the stopped copy, replaced
	a2.inject("logs/disruptions/entries/", stopAfter)
	after := editDoc("e2", "draft", 1, 200)
	if code, _, _ := httpPost(t, baseA2+"/edits", after); code == 200 {
		t.Fatal("a copy stopped after the create acknowledged the write")
	}
	if code, _, body := httpPost(t, baseB+"/edits", after); code != 200 || !strings.Contains(body, `"e2"`) {
		t.Fatalf("sent again to the other copy: %d %s", code, body)
	}

	_, a3, baseA3 := copyOver(t, shared, "")
	a3.inject("logs/notes/entries/", stopAfter)
	note := map[string]any{"key": "tablet/1", "ops": []any{writeNote("n1", "No operator.")}}
	if code, _ := intentPost(t, baseA3, "notes", note); code == 200 {
		t.Fatal("a copy stopped after the create acknowledged the note")
	}
	if code, out := intentPost(t, baseB, "notes", note); code != 200 || out["landed"] != true {
		t.Fatalf("the other copy answers the entry it landed in: %d %v", code, out)
	}

	// A commit stopped between its blob and its record leaves an orphan
	// blob for GC, and the resend commits once.
	_, a4, _ := copyOver(t, shared, "")
	a4.inject("pointers/policy/", stopBefore)
	env := store.Envelope{Kind: "policy", SchemaVersion: 1, ProducedBy: "server"}
	stopped := store.NewServing(a4)
	if _, _, err := stopped.CAS("policy", "", []byte(`{"p":1}`), env, "", "w", 1); err == nil {
		t.Fatal("a copy stopped before the record committed it")
	}
	other := store.NewServing(shared)
	if _, seq, err := other.CAS("policy", "", []byte(`{"p":1}`), env, "", "w", 2); err != nil || seq != 1 {
		t.Fatalf("the other copy commits it once: %d %v", seq, err)
	}

	onceEach(t, disruptionsLog, changedIDs(t, shared, disruptionsLog), []string{"e1", "e2"})
	onceEach(t, "notes", changedIDs(t, shared, "notes"), []string{"n1"})
	time.Sleep(store.FreshnessWindow + 100*time.Millisecond)
	if _, _, body := httpGet(t, baseB+"/edits"); !strings.Contains(body, `"e1"`) || !strings.Contains(body, `"e2"`) {
		t.Fatalf("the other copy serves both: %s", body)
	}
	if report, err := store.Verify(context.Background(), shared); err != nil || report.Orphans > 1 {
		t.Fatalf("store verify after the stops (the one orphan is the stopped commit's blob): %+v %v", report, err)
	}
}

// Pins that a slow bucket that answers within the timeout is a bucket
// that answers. A slower one is store/diskcache_test.go.
func TestASlowBucketWithinTheTimeoutKeepsEveryWriteOnce(t *testing.T) {
	shared := store.NewMemory()
	s, view, base := copyOver(t, shared, "")
	view.delay.Store(int64(20 * time.Millisecond))
	var wg sync.WaitGroup
	var ids []string
	for i := 0; i < 4; i++ {
		ids = append(ids, editID(0, i))
		wg.Add(1)
		go func() {
			defer wg.Done()
			if code, _, body := httpPost(t, base+"/edits", editDoc(editID(0, i), "draft", 1, 100)); code != 200 {
				t.Errorf("a write over a slow bucket: %d %s", code, body)
			}
		}()
	}
	wg.Wait()
	if code, rd := readyz(t, s); code != 200 || rd.Status != "ready" {
		t.Fatalf("slow is ready: %d %+v", code, rd)
	}
	view.delay.Store(0)
	onceEach(t, disruptionsLog, changedIDs(t, shared, disruptionsLog), ids)
}
