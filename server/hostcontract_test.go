package main

// The host contract suite, the server's side
// (web/src/uis/host-contract.json; docs/architecture/hosts.md, "The
// rules every host keeps"). Each scenario runs GET /edits with the
// bucket as the server's upstream. The browser runs the same scenarios
// with the server as its upstream (web/src/uis/host-contract.test.mjs).
// Both hosts must give the same state for each step: current, stale
// with the upstream's age (X-Store-Stale, X-Store-As-Of), or failed.

import (
	"context"
	"encoding/json"
	"net/http/httptest"
	"os"
	"strconv"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"rustwasm/server/store"
)

type contractStep struct {
	Upstream string `json:"upstream"`
	Head     string `json:"head"`
	Expect   struct {
		State string `json:"state"`
		Head  string `json:"head"`
		AsOf  string `json:"as_of"`
	} `json:"expect"`
}

type contractScenario struct {
	Name  string         `json:"name"`
	Steps []contractStep `json:"steps"`
}

// slowBucket answers after the store's time limit while `slow` is set,
// and not at all while `down` is set.
type slowBucket struct {
	flakyBucket
	slow atomic.Bool
}

func (b *slowBucket) wait(ctx context.Context) error {
	if !b.slow.Load() {
		return nil
	}
	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-time.After(time.Second):
		return nil
	}
}

func (b *slowBucket) Get(ctx context.Context, k string) ([]byte, map[string]string, error) {
	if err := b.wait(ctx); err != nil {
		return nil, nil, err
	}
	return b.flakyBucket.Get(ctx, k)
}

func (b *slowBucket) Stat(ctx context.Context, k string) (store.ObjectInfo, error) {
	if err := b.wait(ctx); err != nil {
		return store.ObjectInfo{}, err
	}
	return b.flakyBucket.Stat(ctx, k)
}

func (b *slowBucket) List(ctx context.Context, p, a string) ([]store.ObjectInfo, error) {
	if err := b.wait(ctx); err != nil {
		return nil, err
	}
	return b.flakyBucket.List(ctx, p, a)
}

// The heads the scenarios name, as edit documents: B is a later
// revision of A's record. The records are submitted already, so the
// server keeps their revs (it submits each draft it receives:
// transit_sync::submit).
var contractHeads = map[string]struct {
	doc string
	rev string
}{
	"A": {editDoc("e1", "submitted", 1, 100), `"rev":1`},
	"B": {editDoc("e1", "submitted", 2, 200), `"rev":2`},
}

func TestTheServerKeepsTheHostContract(t *testing.T) {
	raw, err := os.ReadFile("../web/src/uis/host-contract.json")
	if err != nil {
		t.Fatal(err)
	}
	var contract struct {
		Scenarios []contractScenario `json:"scenarios"`
	}
	if err := json.Unmarshal(raw, &contract); err != nil {
		t.Fatal(err)
	}
	restore := store.SetCallTimeouts(200*time.Millisecond, 200*time.Millisecond)
	t.Cleanup(restore)
	for _, sc := range contract.Scenarios {
		t.Run(sc.Name, func(t *testing.T) { runContract(t, sc) })
	}
}

func runContract(t *testing.T, sc contractScenario) {
	bucket := &slowBucket{flakyBucket: flakyBucket{Backend: store.NewMemory()}}
	// A scenario that starts with the upstream silent starts a server
	// that never read the bucket: it holds no copy.
	if len(sc.Steps) > 0 && sc.Steps[0].Upstream != "answers" {
		bucket.down.Store(true)
	}
	base := httptest.NewServer(storeMux(newStoreServer(t, bucket))).URL
	var lastAnswerMs int64
	var posted string
	for i, step := range sc.Steps {
		where := sc.Name + ", step " + strconv.Itoa(i+1)
		switch step.Upstream {
		case "answers":
			bucket.down.Store(false)
			bucket.slow.Store(false)
			time.Sleep(store.FreshnessWindow + 100*time.Millisecond) // a probe is due
			lastAnswerMs = time.Now().UnixMilli()
			if step.Head != posted {
				if code, _, body := httpPost(t, base+"/edits", contractHeads[step.Head].doc); code != 200 {
					t.Fatalf("%s: post %s: %d %s", where, step.Head, code, body)
				}
				posted = step.Head
			}
		case "silent":
			bucket.down.Store(true)
			time.Sleep(store.FreshnessWindow + 100*time.Millisecond)
		case "slow":
			bucket.slow.Store(true)
			time.Sleep(store.FreshnessWindow + 100*time.Millisecond)
		default:
			t.Fatalf("%s: an upstream the suite does not know: %q", where, step.Upstream)
		}
		silentFrom := time.Now().UnixMilli()
		code, hdr, body := httpGet(t, base+"/edits")
		state := "current"
		switch {
		case code >= 500:
			state = "failed"
		case hdr.Get("X-Store-Stale") == "true":
			state = "stale"
		case code != 200:
			t.Fatalf("%s: GET /edits answered %d", where, code)
		}
		if state != step.Expect.State {
			t.Fatalf("%s: the server says %s, the contract %s (%d %v)", where, state, step.Expect.State, code, hdr)
		}
		if step.Expect.Head != "" && !strings.Contains(body, contractHeads[step.Expect.Head].rev) {
			t.Fatalf("%s: the server serves another head than %s: %s", where, step.Expect.Head, body)
		}
		if step.Expect.AsOf == "last-answer" {
			asOf, err := strconv.ParseInt(hdr.Get("X-Store-As-Of"), 10, 64)
			if err != nil || asOf < lastAnswerMs || asOf > silentFrom {
				t.Fatalf("%s: the age is when the bucket last answered (%d..%d), not %q", where, lastAnswerMs, silentFrom, hdr.Get("X-Store-As-Of"))
			}
		}
	}
}
