package main

// The kind table governs the generic pointer write (D-202). The laws:
//   - a kind with no entry is refused, and nothing is written;
//   - a kind with its own route is refused, and the answer names the
//     route;
//   - a pointer kind heads only its pointers;
//   - a pointer kind is checked by its module: what makes it wrong is a
//     422 that names why, and a consequence is accepted and named;
//   - the store holds the module's canonical bytes, produced by it;
//   - a restricted kind is never served by hash;
//   - every blob the server writes is of a kind the table names.

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"rustwasm/server/store"
)

func pointerPost(t *testing.T, base, name, kind, doc string) (int, string) {
	t.Helper()
	req, _ := json.Marshal(map[string]any{"kind": kind, "document": doc})
	code, _, body := httpPost(t, base+"/pointers/"+name, string(req))
	return code, body
}

func TestTheKindTableGovernsThePointerWrite(t *testing.T) {
	bucket := store.NewMemory()
	s := newStoreServer(t, bucket)
	base := httptest.NewServer(storeMux(s)).URL
	sound := `{"schema_version":1,"rules":[{"name":"admin","roles":["admin"],"pointers":["*"],"actions":["*"]}]}`

	for _, c := range []struct{ name, kind, doc, want string }{
		{"scratch", "scratch", `{}`, "keeps no kind"},
		{"stops-copy", "stops", `{}`, "POST /stops"},
		{"manifest-2", "manifest", `{}`, "the server's publishing"},
		{"rules", "policy", sound, "heads only the pointers policy"},
		{"build.x", "policy", sound, "holds a build document"},
	} {
		code, body := pointerPost(t, base, c.name, c.kind, c.doc)
		if code != http.StatusBadRequest || !strings.Contains(body, c.want) {
			t.Fatalf("%s %s: %d %s, want 400 naming %q", c.kind, c.name, code, body, c.want)
		}
		if _, found, _ := s.store.Head(c.name); found {
			t.Fatalf("%s: a refused write writes nothing", c.name)
		}
	}

	code, body := pointerPost(t, base, "policy", "policy", `{"schema_version":1,"rules":[{"name":"ops","roles":["d"],"pointers":["edits"],"actions":["wirte"]}]}`)
	if code != http.StatusUnprocessableEntity || !strings.Contains(body, "unknown_action") {
		t.Fatalf("a wrong policy: %d %s", code, body)
	}
	if _, found, _ := s.store.Head("policy"); found {
		t.Fatal("a refused policy writes nothing")
	}

	code, body = pointerPost(t, base, "policy", "policy", `{"schema_version":1,"rules":[{"name":"ops","roles":["d"],"pointers":["edits"],"actions":["write"]}]}`)
	if code != 200 || !strings.Contains(body, "no_policy_writer") {
		t.Fatalf("a policy nobody can change is accepted and named: %d %s", code, body)
	}
	head, _, _ := s.store.Head("policy")
	stored, env, _ := s.store.GetBlob(head.Hash)
	if !strings.HasPrefix(env.ProducedBy, "cap-authz@") || !strings.Contains(string(stored), `"kind":"policy"`) {
		t.Fatalf("the store holds the module's canonical bytes: %+v %s", env, stored)
	}
	if code, _, _ := httpGet(t, base+"/blobs/"+head.Hash); code != http.StatusNotFound {
		t.Fatalf("a restricted kind is never served by hash: %d", code)
	}
}

// Every blob a server writes, booted with a feed, a street layer, a
// published pick, and the edits, is of a kind the table names.
func TestEveryBlobTheServerWritesIsAKindTheTableNames(t *testing.T) {
	bucket := store.NewMemory()
	s := derivedServer(t, bucket, writeLayer(t))
	s.plans = newPlansHost(t)
	s.networkWasm = networkWasm
	s.fixedToday = "2026-08-01"
	if _, err := s.buildManifest(); err != nil {
		t.Fatal(err)
	}
	body, _ := json.Marshal(fixtureNetwork(t, "fall-2026", "Fall 2026", "2026-09-07"))
	if _, refused := s.publishNetwork(context.Background(), body, "anonymous"); refused != nil {
		t.Fatal(refused)
	}
	srv := httptest.NewServer(storeMux(s))
	t.Cleanup(srv.Close)
	if code, _, out := httpPost(t, srv.URL+"/edits", editDoc("e1", "draft", 1, 100)); code != 200 {
		t.Fatalf("edits: %d %s", code, out)
	}
	httpGet(t, srv.URL+"/manifest")

	table, err := s.kindTable()
	if err != nil {
		t.Fatal(err)
	}
	blobs, err := bucket.List(context.Background(), "blobs/", "")
	if err != nil || len(blobs) == 0 {
		t.Fatalf("blobs: %d %v", len(blobs), err)
	}
	seen := map[string]bool{}
	for _, o := range blobs {
		_, env, err := s.store.GetBlob("sha256:" + strings.TrimPrefix(o.Key, "blobs/"))
		if err != nil {
			t.Fatal(err)
		}
		seen[env.Kind] = true
		if table.byStoreKind(env.Kind) == nil {
			t.Errorf("the server wrote a %s blob, and the kind table names no such kind", env.Kind)
		}
	}
	for _, want := range []string{"network_document", "boundary", "streets", "walks", "derivation", "gtfs_feed", "street_layer", "manifest"} {
		if !seen[want] {
			t.Errorf("the boot wrote no %s blob: %v", want, seen)
		}
	}
	// The store describes itself (D-203), and verifies with every
	// marked reference followed.
	r, err := store.Verify(context.Background(), bucket)
	if err != nil || r.Schemas == 0 || r.Tables != 1 {
		t.Fatalf("a booted store describes itself and verifies: %+v %v", r, err)
	}
	if _, _, err := bucket.Get(context.Background(), store.ReadmeKey); err != nil {
		t.Fatalf("the README: %v", err)
	}
}
