package main

// Pins the instances of the server's one module (
// docs/architecture/server.md, "The instances"): authz answers through
// a trap in the long-lived instance, an import's instance is gone after
// the import, and a refresh answers every request with no wait and
// swaps cleanly.

import (
	"context"
	"crypto/rand"
	"crypto/rsa"
	"encoding/json"
	"net/http/httptest"
	"testing"
	"time"

	"rustwasm/server/store"
	"rustwasm/server/wasmhost"
)

func TestATrapDuringAWriteLeavesAuthzAnswering(t *testing.T) {
	ctx := context.Background()
	rsaKey, _ := rsa.GenerateKey(rand.Reader, 2048)
	jwks := newJWKSServer([]map[string]any{rsaJWK("k1", &rsaKey.PublicKey)})
	defer jwks.srv.Close()
	s, base := newAuthServer(t, jwks)
	inst, err := s.fresh(ctx)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { inst.Close(ctx) })
	s.authz.wasm = wasmhost.NewHeld("authz", inst).As("authz")
	t.Cleanup(func() { s.long.Wait() })

	now := time.Now().Unix()
	token := func(role string) string {
		return signRS256(t, rsaKey, "k1", map[string]any{
			"iss": "https://idp.example", "aud": "transit-server", "sub": role + "-1",
			"roles": []string{role}, "exp": now + 3600, "nbf": now - 10,
		})
	}
	trapped := s.longLived().Current()
	s.longLived().Locked(func() {
		if err := trapped.Fault(ctx); err == nil {
			t.Error("the fault did not trap")
		}
	})
	if code, body := bearerPost(t, base+"/edits", token("dispatcher"), editDoc("e1", "draft", 1, 100)); code == 200 {
		t.Fatalf("a write on the trapped instance: %d %s", code, body)
	}
	if code, _ := bearerPost(t, base+"/edits", token("rider"), editDoc("e1", "draft", 1, 100)); code != 403 {
		t.Fatalf("authz answers through the trap: want 403 for a rider, got %d", code)
	}
	s.long.Wait()
	if s.long.Current() == trapped {
		t.Fatal("the trapped instance was not replaced")
	}
	if code, body := bearerPost(t, base+"/edits", token("dispatcher"), editDoc("e1", "draft", 1, 100)); code != 200 {
		t.Fatalf("a write after the replacement: %d %s", code, body)
	}
}

func TestAnImportsInstanceIsGoneAfterTheImport(t *testing.T) {
	ctx := context.Background()
	if _, err := NewWasmHost(ctx, testWasm); err != nil {
		t.Fatal(err) // compiled before the count
	}
	before := wasmhost.Live()
	if _, _, _, err := importFeed(ctx, testWasm, fixtureZip(t)); err != nil {
		t.Fatal(err)
	}
	if after := wasmhost.Live(); after != before {
		t.Fatalf("instances open: %d before the import, %d after", before, after)
	}
}

func TestARefreshAnswersEveryRequestAndSwapsCleanly(t *testing.T) {
	s := newStoreServer(t, store.NewMemory())
	closeModules(t, s)
	mux := storeMux(s)
	mux.HandleFunc("/notes", s.kindRoutes().HandleKindPost)
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	trip := map[string]any{"kind": "trip", "trip": "t10", "date": "2026-10-23", "start_secs": 28800}
	post := func(id, text string) int {
		body, _ := json.Marshal(map[string]any{"id": id, "target": trip, "text": text})
		code, _, resp := httpPost(t, srv.URL+"/notes", string(body))
		if code != 200 && code != 422 {
			t.Fatalf("note %s: %d %s", id, code, resp)
		}
		return code
	}
	long := s.longLived()
	if post("n1", "No operator.") != 200 {
		t.Fatal("the first note")
	}
	reached, release := make(chan struct{}), make(chan struct{})
	long.Hook = func(stage string) {
		if stage == "registered" {
			close(reached)
			<-release
		}
	}
	long.SetCeiling(0)
	old := long.Current()
	if post("n2", "Late.") != 200 {
		t.Fatal("the note that starts the refresh")
	}
	<-reached
	long.SetCeiling(wasmhost.MemoryCeiling)

	started := time.Now()
	if post("n3", "Covered.") != 200 {
		t.Fatal("a note during the refresh")
	}
	if waited := time.Since(started); waited > 2*time.Second {
		t.Fatalf("a request during the refresh waited %v", waited)
	}
	if long.Current() != old {
		t.Fatal("the old instance answers until the swap")
	}
	close(release)
	long.Wait()
	if long.Current() == old {
		t.Fatal("no swap")
	}
	if err := old.As("notes").Invoke(context.Background(), map[string]any{"op": "about"}, nil); err != wasmhost.ErrClosed {
		t.Fatalf("the old instance answers nothing after the swap: %v", err)
	}

	// The head that moved during the refresh (n3) was registered before the
	// swap, so a write after it registers nothing.
	loads, heads := long.Loads()
	if heads == 0 {
		t.Fatal("the long-lived instance holds no head")
	}
	if post("n4", " ") != 422 {
		t.Fatal("a blank note is refused")
	}
	if after, _ := long.Loads(); after != loads {
		t.Fatalf("a write after the swap registered %d heads the instance held", after-loads)
	}
}
