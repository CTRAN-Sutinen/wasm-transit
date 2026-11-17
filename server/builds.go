package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"log"
	"mime"
	"net/http"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"

	"rustwasm/server/store"
)

// Builds (D-80; docs/architecture/release-artifacts.md;
// docs/FORMATS.md, "Builds"). In store mode the release in -dist is a
// BUILD: every file a blob, a build document naming them and the
// modules this server hosts, committed to the `build` pointer. The
// manifest pins the head (`build_hash`) and the pages are served from
// it, so every build the log ever held stays whole, and moving the
// pointer back (POST /builds) is a rollback the pages follow (D-63).
//
// -dist is read like a source (D-118): at most every buildEvery, a
// changed tree is read whole, and committed when its build is new to
// the log. A build the log already holds is never re-imported — moving
// to it is POST /builds — so a restart on the release a rollback left
// behind keeps the rollback.

// buildPointer is the pointer the server's builds are logged under.
const buildPointer = "build"

// Each UI at its own URL (D-171): a UI's bundle (-ui-dist <id>=<dir>,
// `just build-ui <id>`) is a build of its own, committed to the pointer
// build.<id> the same way, with no server modules. The UI's host
// serves it (`just ui-deploy` writes a logged build out for it) and
// routes the data paths here. POST /builds?ui=<id> moves that UI's
// pointer alone.

// uiBuildPointer is the pointer a UI's builds are logged under.
func uiBuildPointer(id string) string { return buildPointer + "." + id }

// validUIID says whether id can name a UI's pointer: lower-case letters,
// digits, and hyphens, as the generated UIs' ids are.
func validUIID(id string) bool {
	if id == "" || len(id) > 64 {
		return false
	}
	for _, c := range id {
		if !(c >= 'a' && c <= 'z' || c >= '0' && c <= '9' || c == '-') {
			return false
		}
	}
	return true
}

// uiDist is one UI's bundle directory and what was last read of it.
type uiDist struct {
	id, dir string
	state   buildState
}

// parseUIDists reads -ui-dist: "<id>=<dir>" pairs, comma-separated.
func parseUIDists(spec string) ([]*uiDist, error) {
	var out []*uiDist
	seen := map[string]bool{}
	for _, part := range strings.Split(spec, ",") {
		part = strings.TrimSpace(part)
		if part == "" {
			continue
		}
		id, dir, ok := strings.Cut(part, "=")
		if !ok || !validUIID(id) || dir == "" {
			return nil, fmt.Errorf("-ui-dist: %q is not <id>=<dir> (an id of a-z, 0-9, and -)", part)
		}
		if seen[id] {
			return nil, fmt.Errorf("-ui-dist: %s twice", id)
		}
		seen[id] = true
		out = append(out, &uiDist{id: id, dir: dir})
	}
	return out, nil
}

var (
	buildEnv     = store.Envelope{Kind: store.BuildKind, SchemaVersion: 1, ProducedBy: "server"}
	buildFileEnv = store.Envelope{Kind: store.BuildFileKind, SchemaVersion: 1, ProducedBy: "server"}
)

// buildEveryDefault bounds how often a request re-reads -dist's listing.
const buildEveryDefault = 2 * time.Second

// buildExcluded: the basemap is map data its own recipe fetches
// (`just basemap`, `just basemap-assets` — gitignored), not a release's
// code; it is served from -dist as it is.
func buildExcluded(rel string) bool {
	return rel == "basemap.pmtiles" || strings.HasPrefix(rel, "basemap-assets/")
}

// builds is the server's view of -dist and the pinned build.
type buildState struct {
	mu          sync.Mutex
	checked     time.Time
	fingerprint string // -dist's listing (paths, sizes, modified times) last read

	pinMu  sync.Mutex
	pinned string // the build the byPath index answers
	byPath map[string]store.BuildFile
}

// distListing walks a bundle directory (the build's files only) and
// answers its relative paths, sorted, and a fingerprint of their sizes
// and times.
func distListing(dist string) ([]string, string, error) {
	var paths []string
	h := sha256.New()
	err := filepath.WalkDir(dist, func(p string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, _ := filepath.Rel(dist, p)
		rel = filepath.ToSlash(rel)
		if d.IsDir() {
			if rel != "." && buildExcluded(rel+"/") {
				return filepath.SkipDir
			}
			return nil
		}
		if buildExcluded(rel) {
			return nil
		}
		info, err := d.Info()
		if err != nil {
			return err
		}
		paths = append(paths, rel)
		fmt.Fprintf(h, "%s\x00%d\x00%d\n", rel, info.Size(), info.ModTime().UnixNano())
		return nil
	})
	if err != nil {
		return nil, "", err
	}
	sort.Strings(paths)
	return paths, hex.EncodeToString(h.Sum(nil)), nil
}

// readBuild reads a bundle directory and the modules named as a build:
// its document and the bytes of every blob it names.
func readBuild(dist string, paths []string, modules map[string]string) ([]byte, map[string][]byte, error) {
	doc := store.BuildDoc{Kind: store.BuildKind, SchemaVersion: 1, Files: []store.BuildFile{}}
	blobs := map[string][]byte{}
	for _, rel := range paths {
		data, err := os.ReadFile(filepath.Join(dist, filepath.FromSlash(rel)))
		if err != nil {
			return nil, nil, err
		}
		h := store.HashOf(data)
		blobs[h] = data
		doc.Files = append(doc.Files, store.BuildFile{Path: rel, Hash: h, Type: contentType(rel)})
	}
	for role, p := range modules {
		data, err := os.ReadFile(p)
		if err != nil {
			return nil, nil, fmt.Errorf("the %s module: %w", role, err)
		}
		h := store.HashOf(data)
		blobs[h] = data
		doc.Server = append(doc.Server, store.BuildModule{Role: role, Hash: h})
	}
	data, err := json.Marshal(doc.Sorted())
	return data, blobs, err
}

func contentType(rel string) string {
	if t := mime.TypeByExtension(path.Ext(rel)); t != "" {
		return t
	}
	return "application/octet-stream"
}

// syncBuild commits -dist's release, and each -ui-dist bundle, when it
// changed and is new to its log (at most every buildEvery). Store mode
// only; a directory that does not read, or holds no page, commits
// nothing.
func (s *server) syncBuild() {
	if s.store == nil {
		return
	}
	if s.dist != "" {
		s.syncBuildOf(buildPointer, s.dist, &s.builds, s.serverModules)
	}
	for _, u := range s.uiDists {
		s.syncBuildOf(uiBuildPointer(u.id), u.dir, &u.state, nil)
	}
}

// syncBuildOf commits one directory's release to its pointer.
func (s *server) syncBuildOf(pointer, dist string, b *buildState, modules map[string]string) {
	b.mu.Lock()
	defer b.mu.Unlock()
	every := s.buildEvery
	if every == 0 {
		every = buildEveryDefault
	}
	if !b.checked.IsZero() && time.Since(b.checked) < every {
		return
	}
	b.checked = time.Now()
	paths, fingerprint, err := distListing(dist)
	if err != nil || fingerprint == b.fingerprint {
		return
	}
	b.fingerprint = fingerprint
	if !containsString(paths, "index.html") {
		return
	}
	doc, blobs, err := readBuild(dist, paths, modules)
	if err != nil {
		log.Printf("build: reading %s: %v (the pinned build stays)", dist, err)
		return
	}
	hash := store.HashOf(doc)
	entries, _, err := s.store.Log(pointer, "")
	if err != nil {
		log.Printf("build: %v", err)
		return
	}
	for _, e := range entries {
		if e.To == hash {
			return // the log holds it: moving to it is POST /builds
		}
	}
	for attempt := 0; attempt < 5; attempt++ {
		from := ""
		if head, found, err := s.store.Head(pointer); err != nil {
			log.Printf("build: %v", err)
			return
		} else if found {
			from = head.Hash
		}
		_, seq, err := s.store.CommitBuild(pointer, from, doc, buildEnv, blobs, buildFileEnv, "import:"+dist, time.Now().UnixMilli())
		var ce *store.ConflictError
		if errors.As(err, &ce) {
			continue
		}
		if err != nil {
			log.Printf("build: commit: %v", err)
			return
		}
		log.Printf("build: %s committed to %s as %s seq %d (%d files)", dist, pointer, hash, seq, len(paths))
		return
	}
}

func containsString(xs []string, x string) bool {
	for _, y := range xs {
		if y == x {
			return true
		}
	}
	return false
}

// pinnedFiles answers the head build's files by path (nil when no build
// is pinned).
func (s *server) pinnedFiles() (string, map[string]store.BuildFile) {
	head, found, err := s.store.Head(buildPointer)
	if err != nil || !found {
		return "", nil
	}
	b := &s.builds
	b.pinMu.Lock()
	defer b.pinMu.Unlock()
	if b.pinned == head.Hash {
		return b.pinned, b.byPath
	}
	data, _, err := s.store.GetBlob(head.Hash)
	if err != nil {
		return "", nil
	}
	doc, err := store.ParseBuild(data)
	if err != nil {
		log.Printf("build: the head %s: %v", head.Hash, err)
		return "", nil
	}
	byPath := make(map[string]store.BuildFile, len(doc.Files))
	for _, f := range doc.Files {
		byPath[f.Path] = f
	}
	b.pinned, b.byPath = head.Hash, byPath
	return b.pinned, b.byPath
}

// serveBuildFile serves name from the pinned build; false when the
// build does not hold it (or none is pinned).
func (s *server) serveBuildFile(w http.ResponseWriter, r *http.Request, name string) bool {
	_, files := s.pinnedFiles()
	f, ok := files[name]
	if !ok {
		return false
	}
	data, _, err := s.store.GetBlob(f.Hash)
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return true
	}
	w.Header().Set("Content-Type", f.Type)
	w.Header().Set("Cache-Control", staticCacheControl(name))
	w.Header().Set("ETag", `"`+f.Hash+`"`)
	http.ServeContent(w, r, name, time.Time{}, bytes.NewReader(data))
	return true
}

// handleBuilds moves the build pointer to a build its log holds — a
// rollback, or forward again: POST {"build": "sha256:<hex>"}. Every
// file of a logged build is retained, so the move is the whole act;
// the manifest moves with it and the pages follow (D-63). With ?ui=<id>
// it moves that UI's pointer (build.<id>) instead, and nothing else.
func (s *server) handleBuilds(w http.ResponseWriter, r *http.Request) {
	if s.store == nil {
		http.Error(w, "no store configured (start with -store <url>)", http.StatusNotFound)
		return
	}
	if r.Method != http.MethodPost {
		http.Error(w, "use POST", http.StatusMethodNotAllowed)
		return
	}
	pointer, ok := buildsPointerOf(r)
	if !ok {
		http.Error(w, "ui: an id of a-z, 0-9, and -", http.StatusBadRequest)
		return
	}
	var req struct {
		Build string `json:"build"`
	}
	body, _ := io.ReadAll(io.LimitReader(r.Body, 1<<16))
	if err := json.Unmarshal(body, &req); err != nil || req.Build == "" {
		http.Error(w, `bad request: want {"build":"sha256:<hex>"}`, http.StatusBadRequest)
		return
	}
	entries, _, err := s.store.Log(pointer, "")
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return
	}
	logged := false
	for _, e := range entries {
		logged = logged || e.To == req.Build
	}
	if !logged {
		http.Error(w, "the "+pointer+" log holds no "+req.Build, http.StatusNotFound)
		return
	}
	data, _, err := s.store.GetBlob(req.Build)
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return
	}
	head, _, err := s.store.Head(pointer)
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return
	}
	_, seq, err := s.store.CAS(pointer, head.Hash, data, buildEnv, req.Build, writerOf(r, "anonymous"), time.Now().UnixMilli())
	if err != nil {
		var ce *store.ConflictError
		if errors.As(err, &ce) {
			http.Error(w, err.Error(), http.StatusConflict)
			return
		}
		fail(w, err, http.StatusInternalServerError)
		return
	}
	log.Printf("build: %s moved to %s (seq %d)", pointer, req.Build, seq)
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]any{"build": req.Build, "seq": seq})
}

// buildsPointerOf answers the pointer a POST /builds moves: build, or
// build.<id> for ?ui=<id>. False for an id that names no UI pointer.
func buildsPointerOf(r *http.Request) (string, bool) {
	ui := r.URL.Query().Get("ui")
	if ui == "" {
		return buildPointer, true
	}
	if !validUIID(ui) {
		return "", false
	}
	return uiBuildPointer(ui), true
}
