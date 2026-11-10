package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"rustwasm/server/store"
)

// Builds (D-80; docs/architecture/release-artifacts.md): in store
// mode the server commits the release in -dist to the `build` pointer —
// every file a blob, the build document naming them and the modules
// the server hosts — the manifest pins it (`build_hash`), and the pages
// are served from the pinned build. A rollback is POST /builds naming
// a build the log holds: the pointer moves back, the manifest moves
// with it, and the pages follow (D-63).

const syncWasmPath = "../web/pkg/cap_sync.wasm"

func writeDist(t *testing.T, dir string, files map[string]string) {
	t.Helper()
	for name, body := range files {
		p := filepath.Join(dir, name)
		os.MkdirAll(filepath.Dir(p), 0o755)
		if err := os.WriteFile(p, []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
}

func newBuildServer(t *testing.T, b store.Backend, dist string) (*server, string) {
	t.Helper()
	s := newStoreServer(t, b)
	s.dist = dist
	s.buildEvery = time.Nanosecond
	s.serverModules = map[string]string{"sync": syncWasmPath}
	s.syncBuild()
	return s, httptest.NewServer(s.routes()).URL
}

func manifestBuild(t *testing.T, base string) string {
	t.Helper()
	_, _, body := httpGet(t, base+"/manifest")
	var m Manifest
	if err := json.Unmarshal([]byte(body), &m); err != nil {
		t.Fatal(err)
	}
	return m.BuildHash
}

func buildLog(t *testing.T, base string) []store.LogEntry {
	t.Helper()
	_, _, body := httpGet(t, base+"/pointers/build/log")
	var log struct {
		Entries []store.LogEntry `json:"entries"`
	}
	json.Unmarshal([]byte(body), &log)
	return log.Entries
}

var releaseA = map[string]string{
	"index.html":               "<p>A</p>",
	"sw.js":                    `const BUILD = "a";`,
	"assets/index-AAAAAAAA.js": "console.log('A')",
	"basemap.pmtiles":          "map data",
}

func TestTheDistIsCommittedAsABuildAndThePagesAreServedFromIt(t *testing.T) {
	dist := t.TempDir()
	writeDist(t, dist, releaseA)
	b := store.NewMemory()
	s, base := newBuildServer(t, b, dist)

	pinned := manifestBuild(t, base)
	entries := buildLog(t, base)
	if pinned == "" || len(entries) != 1 || entries[0].To != pinned || entries[0].Kind != store.BuildKind {
		t.Fatalf("the manifest pins the one build the log holds: %q %+v", pinned, entries)
	}
	data, _, err := s.store.GetBlob(pinned)
	if err != nil {
		t.Fatal(err)
	}
	doc, err := store.ParseBuild(data)
	if err != nil {
		t.Fatal(err)
	}
	paths := []string{}
	for _, f := range doc.Files {
		paths = append(paths, f.Path)
	}
	if strings.Join(paths, ",") != "assets/index-AAAAAAAA.js,index.html,sw.js" {
		t.Fatalf("the build is the release's files — the basemap is map data, not the release: %v", paths)
	}
	if len(doc.Server) != 1 || doc.Server[0].Role != "sync" || doc.Server[0].Hash != s.wasm.Hash() {
		t.Fatalf("the build retains the modules the server hosts: %+v", doc.Server)
	}

	// A replacement process given only the bucket (its -dist empty)
	// serves the same pages, from the store.
	_, base2 := newBuildServer(t, b, t.TempDir())
	for path, want := range map[string]string{"/": "<p>A</p>", "/sw.js": `const BUILD = "a";`, "/assets/index-AAAAAAAA.js": "console.log('A')"} {
		code, hdr, body := httpGet(t, base2+path)
		if code != 200 || body != want {
			t.Fatalf("%s from the store: %d %q", path, code, body)
		}
		if path == "/assets/index-AAAAAAAA.js" && hdr.Get("Cache-Control") != "public, max-age=31536000, immutable" {
			t.Fatalf("a hashed file is still immutable: %q", hdr.Get("Cache-Control"))
		}
		if path == "/sw.js" && (hdr.Get("Cache-Control") != "no-cache" || hdr.Get("ETag") == "") {
			t.Fatalf("a page revalidates by its blob: %v", hdr)
		}
	}
	if got := manifestBuild(t, base2); got != pinned {
		t.Fatalf("an empty -dist commits nothing: %q != %q", got, pinned)
	}
	if code, _, body := httpGet(t, base+"/basemap.pmtiles"); code != 200 || body != "map data" {
		t.Fatalf("the basemap is served from -dist as it is: %d %q", code, body)
	}
}

func TestANewReleaseIsANewBuildAndARollbackServesTheOldBytes(t *testing.T) {
	dist := t.TempDir()
	writeDist(t, dist, releaseA)
	b := store.NewMemory()
	_, base := newBuildServer(t, b, dist)
	a := manifestBuild(t, base)

	// Release B lands in -dist: the next request commits it.
	writeDist(t, dist, map[string]string{"index.html": "<p>B</p>", "sw.js": `const BUILD = "b";`})
	bHash := manifestBuild(t, base)
	if bHash == a || len(buildLog(t, base)) != 2 {
		t.Fatalf("B is the head and A stays in the log: %q %+v", bHash, buildLog(t, base))
	}
	if _, _, body := httpGet(t, base+"/sw.js"); body != `const BUILD = "b";` {
		t.Fatalf("the pages are B's: %q", body)
	}

	// Roll back: the pointer moves to A, and A's bytes are served.
	code, _, body := httpPost(t, base+"/builds", `{"build":"`+a+`"}`)
	if code != 200 {
		t.Fatalf("rollback: %d %s", code, body)
	}
	if got := manifestBuild(t, base); got != a {
		t.Fatalf("the manifest pins A again: %q", got)
	}
	for path, want := range map[string]string{"/sw.js": `const BUILD = "a";`, "/": "<p>A</p>"} {
		if _, _, body := httpGet(t, base+path); body != want {
			t.Fatalf("%s is A's after the rollback: %q", path, body)
		}
	}
	entries := buildLog(t, base)
	if len(entries) != 3 || entries[2].From != bHash || entries[2].To != a {
		t.Fatalf("the rollback is a logged move: %+v", entries)
	}

	// A restart over the same bucket with B still in -dist keeps A: a
	// build the log already holds is a pointer move, never an import.
	_, base2 := newBuildServer(t, b, dist)
	if got := manifestBuild(t, base2); got != a {
		t.Fatalf("a restart does not undo the rollback: %q", got)
	}
	// And forward again is the same move.
	if code, _, body := httpPost(t, base2+"/builds", `{"build":"`+bHash+`"}`); code != 200 {
		t.Fatalf("roll forward: %d %s", code, body)
	}
	if _, _, body := httpGet(t, base2+"/sw.js"); body != `const BUILD = "b";` {
		t.Fatalf("B again: %q", body)
	}
}

func TestOnlyALoggedBuildIsMovedTo(t *testing.T) {
	dist := t.TempDir()
	writeDist(t, dist, releaseA)
	s, base := newBuildServer(t, store.NewMemory(), dist)
	stray, err := s.store.PutBlob([]byte(`{"kind":"build","schema_version":1,"files":[]}`), store.Envelope{Kind: "build", SchemaVersion: 1, ProducedBy: "server"}, "")
	if err != nil {
		t.Fatal(err)
	}
	for _, body := range []string{`{"build":"` + stray + `"}`, `{"build":"sha256:` + strings.Repeat("0", 64) + `"}`, `{}`} {
		if code, _, resp := httpPost(t, base+"/builds", body); code != http.StatusNotFound && code != http.StatusBadRequest {
			t.Fatalf("%s: a build the log never held is refused: %d %s", body, code, resp)
		}
	}
	req, _ := json.Marshal(map[string]any{"from": manifestBuild(t, base), "kind": "build", "document": `{"kind":"build","schema_version":1,"files":[]}`})
	if code, _, resp := httpPost(t, base+"/pointers/build", string(req)); code != http.StatusBadRequest {
		t.Fatalf("a raw write cannot name files the store does not hold: %d %s", code, resp)
	}
	if len(buildLog(t, base)) != 1 {
		t.Fatal("nothing moved")
	}
}
