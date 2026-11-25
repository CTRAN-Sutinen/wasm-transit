package main

// The derived documents in the record (D-199).

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"

	"rustwasm/server/store"
)

const overnightFeed = "../web/e2e/fixtures/overnight-gtfs.zip"

func derivedServer(t *testing.T, bucket store.Backend, layer string) *server {
	t.Helper()
	return derivedServerOf(t, bucket, sampleFeed, layer)
}

func derivedServerOf(t *testing.T, bucket store.Backend, feed, layer string) *server {
	t.Helper()
	s := newStoreServer(t, bucket)
	s.feeder = &feedSource{src: newSource(feed, "source.feed", "gtfs_feed", s.store), wasm: testWasm}
	s.streets = &streetsSource{src: newSource(layer, "source.streets", "street_layer", s.store), wasm: testWasm}
	return s
}

func manifestOf(t *testing.T, s *server) Manifest {
	t.Helper()
	m, err := s.buildManifest()
	if err != nil {
		t.Fatal(err)
	}
	return m
}

func copyFile(t *testing.T, from string) string {
	t.Helper()
	body, err := os.ReadFile(from)
	if err != nil {
		t.Fatal(err)
	}
	to := t.TempDir() + "/" + strings.ReplaceAll(from, "/", "_")
	if err := os.WriteFile(to, body, 0o644); err != nil {
		t.Fatal(err)
	}
	return to
}

func TestABootOverStoredDerivationsRunsNoModule(t *testing.T) {
	bucket := store.NewMemory()
	layer := writeLayer(t)
	first := derivedServer(t, bucket, layer)
	want := manifestOf(t, first)
	if first.feeder.imports != 1 || first.streets.derivations != 2 {
		t.Fatalf("the first boot derives each document once: %d imports, %d streets derivations", first.feeder.imports, first.streets.derivations)
	}

	second := derivedServer(t, bucket, layer)
	got := manifestOf(t, second)
	if second.feeder.imports != 0 || second.streets.derivations != 0 {
		t.Fatalf("a boot over the stored derivations runs no module: %d imports, %d streets derivations", second.feeder.imports, second.streets.derivations)
	}
	if manifestKeyOf(got) != manifestKeyOf(want) {
		t.Fatalf("the same manifest: %+v, want %+v", got, want)
	}
	a, _ := first.feeder.Load()
	b, _ := second.feeder.Load()
	if !bytes.Equal(a.document, b.document) || !bytes.Equal(a.boundary, b.boundary) || a.timezone != b.timezone {
		t.Fatal("the stored network and boundary are the imported ones")
	}
	var answerA, answerB map[string]any
	json.Unmarshal(a.network, &answerA)
	json.Unmarshal(b.network, &answerB)
	if ja, _ := json.Marshal(answerA); !bytes.Equal(ja, must(json.Marshal(answerB))) {
		t.Fatal("GET /network answers the import's answer, notes and all")
	}
	sa, _ := first.publishedStreets()
	sb, _ := second.publishedStreets()
	if !bytes.Equal(sa.streets.body, sb.streets.body) || !bytes.Equal(sa.walks.body, sb.walks.body) {
		t.Fatal("the stored streets and walk table are the derived ones")
	}
}

func must(b []byte, err error) []byte {
	if err != nil {
		panic(err)
	}
	return b
}

func TestEveryDocumentTheManifestNamesIsStoredWithItsModule(t *testing.T) {
	bucket := store.NewMemory()
	s := derivedServer(t, bucket, writeLayer(t))
	m := manifestOf(t, s)
	srv := httptest.NewServer(storeMux(s))
	t.Cleanup(srv.Close)
	httpGet(t, srv.URL+"/manifest") // commits the manifest pointer
	for _, c := range []struct{ hash, kind, module string }{
		{m.NetworkHash, "network_document", "cap-network@"},
		{m.BoundaryHash, "boundary", "cap-network@"},
		{m.StreetsHash, "streets", "cap-streets@"},
		{m.WalksHash, "walks", "cap-streets@"},
		{m.FeedHash, "gtfs_feed", "server"},
	} {
		_, env, err := s.store.GetBlob(c.hash)
		if err != nil {
			t.Fatalf("%s %s is not stored: %v", c.kind, c.hash, err)
		}
		if env.Kind != c.kind || !strings.HasPrefix(env.ProducedBy, c.module) {
			t.Fatalf("%s: envelope %+v", c.kind, env)
		}
	}
	if _, err := store.Verify(context.Background(), bucket); err != nil {
		t.Fatalf("store verify passes over a booted store: %v", err)
	}
}

func TestAChangedInputIsDerivedOnce(t *testing.T) {
	bucket := store.NewMemory()
	layer := writeLayer(t)
	feed := copyFile(t, sampleFeed)
	first := derivedServerOf(t, bucket, feed, layer)
	before := manifestOf(t, first)

	overnight, _ := os.ReadFile(overnightFeed)
	if err := os.WriteFile(feed, overnight, 0o644); err != nil {
		t.Fatal(err)
	}
	second := derivedServerOf(t, bucket, feed, layer)
	after := manifestOf(t, second)
	if second.feeder.imports != 1 {
		t.Fatalf("a new zip is imported once: %d imports", second.feeder.imports)
	}
	if second.streets.derivations != 1 {
		t.Fatalf("the unchanged layer is read from the store, and the walk table over the new network is derived: %d derivations", second.streets.derivations)
	}
	if after.FeedHash == before.FeedHash || after.NetworkHash == before.NetworkHash || after.StreetsHash != before.StreetsHash {
		t.Fatalf("the new network over the same streets: %+v then %+v", before, after)
	}
	third := derivedServerOf(t, bucket, feed, layer)
	manifestOf(t, third)
	if third.feeder.imports != 0 || third.streets.derivations != 0 {
		t.Fatalf("the new derivations are stored: %d imports, %d derivations", third.feeder.imports, third.streets.derivations)
	}
	if log, _, _ := second.store.Log("network", ""); len(log) != 2 || log[0].To != before.NetworkHash || log[1].To != after.NetworkHash {
		t.Fatalf("the network pointer logs each import: %+v", log)
	}
}

func TestADocumentStoredInAnotherVersionIsDerivedAgain(t *testing.T) {
	st := store.New(store.NewMemory())
	feed := "sha256:" + strings.Repeat("a", 64)
	doc := []byte(`{"kind":"network","schema_version":1}`)
	commitDerivation(st, derivedNetworkPointer, []string{feed}, "cap-network@sha256:"+strings.Repeat("b", 64), json.RawMessage(`{}`),
		derivedDoc{tag: "network", storeKind: "network_document", version: 1, body: doc, hash: store.HashOf(doc)})
	if _, _, ok := storedDerivation(st, derivedNetworkPointer, []string{feed}, "network"); ok {
		t.Fatal("a network stored in version 1 is not read by a build that writes version 2")
	}
	if _, _, ok := storedDerivation(st, derivedNetworkPointer, []string{"sha256:" + strings.Repeat("c", 64)}, "network"); ok {
		t.Fatal("a derivation of other inputs is not read")
	}
}

func TestOnlyTheServersDerivationsWriteTheDerivedPointers(t *testing.T) {
	s := newStoreServer(t, store.NewMemory())
	srv := httptest.NewServer(storeMux(s))
	t.Cleanup(srv.Close)
	for _, c := range []struct{ name, kind string }{
		{"network", "network_document"},
		{"boundary", "boundary"},
		{"streets", "streets"},
		{"walks", "walks"},
		{derivedNetworkPointer, "derivation"},
		{"other", "derivation"},
		{"other", "walks"},
	} {
		body, _ := json.Marshal(map[string]string{"kind": c.kind, "document": `{}`})
		if code, _, text := httpPost(t, srv.URL+"/pointers/"+c.name, string(body)); code != http.StatusBadRequest {
			t.Fatalf("POST /pointers/%s (kind %s): %d %s", c.name, c.kind, code, text)
		}
	}
}
