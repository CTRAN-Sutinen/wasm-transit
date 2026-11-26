// Package builds keeps each release as a build in the store
// (docs/architecture/release-artifacts.md): -dist as the `build` pointer,
// each -ui-dist bundle as its own build.<id>, and POST /builds to move to
// another logged build. The server serves the pinned build's files.
package builds

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
	"regexp"
	"sort"
	"strings"
	"sync"
	"time"

	"rustwasm/server/httpx"
	"rustwasm/server/store"
)

// Builds are the server's builds. With no store, there are none.
type Builds struct {
	store   func() *store.Store
	dist    string
	modules map[string]string
	every   time.Duration
	uis     []*UI
	main    State
}

// New keeps -dist (dist), each -ui-dist bundle (uis), and the server's
// modules (role to path), and re-lists a directory at most once each every
// (0 means buildEveryDefault).
func New(st func() *store.Store, dist string, modules map[string]string, every time.Duration, uis []*UI) *Builds {
	return &Builds{store: st, dist: dist, modules: modules, every: every, uis: uis}
}

// In store mode the release in -dist is a build
// (docs/architecture/release-artifacts.md). A build the log already
// holds is never re-imported. Moving to it is POST /builds, so a
// restart on the release that a rollback left behind keeps the
// rollback.

const Pointer = "build"

// Each -ui-dist bundle is a build of its own, committed to the pointer
// build.<id> with no server modules.
func UIPointer(id string) string { return Pointer + "." + id }

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

type UI struct {
	ID, Dir string
	State   State
}

func ParseUIs(spec string) ([]*UI, error) {
	var out []*UI
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
		out = append(out, &UI{ID: id, Dir: dir})
	}
	return out, nil
}

var (
	buildEnv     = store.Envelope{Kind: store.BuildKind, SchemaVersion: 1, ProducedBy: "server"}
	buildFileEnv = store.Envelope{Kind: store.BuildFileKind, SchemaVersion: 1, ProducedBy: "server"}
)

const buildEveryDefault = 2 * time.Second

// buildExcluded skips the basemap. It is map data that its own recipe
// fetches (`just basemap`, `just basemap-assets`), not release code.
func buildExcluded(rel string) bool {
	return rel == "basemap.pmtiles" || strings.HasPrefix(rel, "basemap-assets/")
}

type State struct {
	mu          sync.Mutex
	checked     time.Time
	fingerprint string // -dist's listing (paths, sizes, modified times) last read

	pinMu  sync.Mutex
	pinned string // the build the byPath index answers
	byPath map[string]store.BuildFile
}

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

func (d *Builds) Sync() {
	if d.store() == nil {
		return
	}
	if d.dist != "" {
		d.syncBuildOf(Pointer, d.dist, &d.main, d.modules)
	}
	for _, u := range d.uis {
		d.syncBuildOf(UIPointer(u.ID), u.Dir, &u.State, nil)
	}
}

func (d *Builds) syncBuildOf(pointer, dist string, b *State, modules map[string]string) {
	b.mu.Lock()
	defer b.mu.Unlock()
	every := d.every
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
	entries, _, err := d.store().Log(pointer, "")
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
		if head, found, err := d.store().Head(pointer); err != nil {
			log.Printf("build: %v", err)
			return
		} else if found {
			from = head.Hash
		}
		_, seq, err := d.store().CommitBuild(pointer, from, doc, buildEnv, blobs, buildFileEnv, "import:"+dist, time.Now().UnixMilli())
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

func (d *Builds) PinnedFiles() (string, map[string]store.BuildFile) {
	head, found, err := d.store().Head(Pointer)
	if err != nil || !found {
		return "", nil
	}
	b := &d.main
	b.pinMu.Lock()
	defer b.pinMu.Unlock()
	if b.pinned == head.Hash {
		return b.pinned, b.byPath
	}
	data, _, err := d.store().GetBlob(head.Hash)
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

func (d *Builds) ServeFile(w http.ResponseWriter, r *http.Request, name string) bool {
	_, files := d.PinnedFiles()
	f, ok := files[name]
	if !ok {
		return false
	}
	data, _, err := d.store().GetBlob(f.Hash)
	if err != nil {
		httpx.Fail(w, err, http.StatusInternalServerError)
		return true
	}
	w.Header().Set("Content-Type", f.Type)
	w.Header().Set("Cache-Control", CacheControl(name))
	w.Header().Set("ETag", `"`+f.Hash+`"`)
	http.ServeContent(w, r, name, time.Time{}, bytes.NewReader(data))
	return true
}

// HandleBuilds moves the pointer to a logged build. Every file of a
// logged build is retained, so the move is the whole rollback, and the
// pages follow the manifest.
func (d *Builds) HandleBuilds(w http.ResponseWriter, r *http.Request) {
	if d.store() == nil {
		http.Error(w, "no store configured (start with -store <url>)", http.StatusNotFound)
		return
	}
	if r.Method != http.MethodPost {
		http.Error(w, "use POST", http.StatusMethodNotAllowed)
		return
	}
	pointer, ok := PointerOf(r)
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
	entries, _, err := d.store().Log(pointer, "")
	if err != nil {
		httpx.Fail(w, err, http.StatusInternalServerError)
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
	data, _, err := d.store().GetBlob(req.Build)
	if err != nil {
		httpx.Fail(w, err, http.StatusInternalServerError)
		return
	}
	head, _, err := d.store().Head(pointer)
	if err != nil {
		httpx.Fail(w, err, http.StatusInternalServerError)
		return
	}
	_, seq, err := d.store().CAS(pointer, head.Hash, data, buildEnv, req.Build, httpx.Writer(r, "anonymous"), time.Now().UnixMilli())
	if err != nil {
		var ce *store.ConflictError
		if errors.As(err, &ce) {
			http.Error(w, err.Error(), http.StatusConflict)
			return
		}
		httpx.Fail(w, err, http.StatusInternalServerError)
		return
	}
	log.Printf("build: %s moved to %s (seq %d)", pointer, req.Build, seq)
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]any{"build": req.Build, "seq": seq})
}

func PointerOf(r *http.Request) (string, bool) {
	ui := r.URL.Query().Get("ui")
	if ui == "" {
		return Pointer, true
	}
	if !validUIID(ui) {
		return "", false
	}
	return UIPointer(ui), true
}

// A file whose name carries its content hash never changes: Vite's
// build output under assets/ (name-<8 chars>.ext) and the
// hash-addressed modules under caps/ (name-<sha256 hex>.wasm).
var (
	viteHashed   = regexp.MustCompile(`^assets/[^/]+-[A-Za-z0-9_-]{8}\.[a-z0-9]+$`)
	moduleHashed = regexp.MustCompile(`^caps/[^/]+-[0-9a-f]{64}\.wasm$`)
)

// CacheControl is the Cache-Control of a build's file. Everything that
// is not content-addressed revalidates on every use, so an open page meets
// the new build on its next load.
func CacheControl(name string) string {
	if viteHashed.MatchString(name) || moduleHashed.MatchString(name) {
		return "public, max-age=31536000, immutable"
	}
	return "no-cache"
}
