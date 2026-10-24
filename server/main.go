// Package main is the sync server: the Go host for the wasm artifact.
//
// The division of labor is deliberate (see the repo README): the
// artifact owns all domain logic — the server loads the exact bytes
// the browser runs (sha256 in the manifest proves it) and calls the
// flat ABI. This host is scaffolding only: HTTP routing, static
// serving with Range support, the CORS proxy, and the manifest that
// pins feed + wasm hashes. No domain decision is ever made here.
package main

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"log"
	"mime"
	"net/http"
	"net/url"
	"os"
	"path"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

func main() {
	addr := flag.String("addr", "127.0.0.1:8080", "listen address")
	dist := flag.String("dist", "../www/dist", "directory of static files to serve")
	wasmPath := flag.String("wasm", "../www/pkg/transit_wasm.wasm", "the wasm artifact to host and pin")
	feedPath := flag.String("feed", "", "path to the GTFS zip (enables /feed and the manifest's feed section)")
	publisher := flag.String("publisher", "", "manifest provenance (who publishes this schedule)")
	flag.Parse()

	mime.AddExtensionType(".wasm", "application/wasm")
	mime.AddExtensionType(".mjs", "text/javascript")
	mime.AddExtensionType(".pmtiles", "application/vnd.pmtiles")

	ctx := context.Background()
	host, err := NewWasmHost(ctx, *wasmPath)
	if err != nil {
		log.Fatalf("wasm host: %v", err)
	}
	log.Printf("wasm artifact loaded: %s (%s)", *wasmPath, host.Hash())

	s := &server{
		dist:      *dist,
		wasm:      host,
		publisher: *publisher,
	}
	if *feedPath != "" {
		s.feeder = &fileFeed{path: *feedPath}
	}

	mux := http.NewServeMux()
	mux.HandleFunc("/rt", s.handleProxy)
	mux.HandleFunc("/manifest", s.handleManifest)
	mux.HandleFunc("/feed", s.handleFeed)
	mux.HandleFunc("/", s.handleStatic)

	log.Printf("listening on http://%s (dist: %s)", *addr, *dist)
	log.Fatal(http.ListenAndServe(*addr, mux))
}

// server holds the host's few pieces of state.
type server struct {
	dist      string
	wasm      *WasmHost
	feeder    *fileFeed // nil when no feed is configured
	publisher string
}

// handleStatic serves the built frontend. http.ServeContent gives
// Range requests (PMTiles needs them), If-Modified-Since, and content
// types by extension — no hand-rolled 206s.
func (s *server) handleStatic(w http.ResponseWriter, r *http.Request) {
	upath := r.URL.Path
	if strings.Contains(upath, "..") {
		http.Error(w, "bad path", http.StatusBadRequest)
		return
	}
	name := path.Clean(strings.TrimPrefix(upath, "/"))
	if name == "" || name == "." {
		name = "index.html"
	}
	file, err := os.Open(filepath.Join(s.dist, name))
	if err != nil {
		http.NotFound(w, r)
		return
	}
	defer file.Close()
	info, err := file.Stat()
	if err != nil || info.IsDir() {
		http.NotFound(w, r)
		return
	}
	// ServeContent sniffs the content type by extension.
	http.ServeContent(w, r, info.Name(), info.ModTime(), file)
}

// handleProxy is the CORS proxy for agency feeds the browser can't
// fetch directly (no Access-Control-Allow-Origin upstream). The
// server is expected to bind localhost in dev; production puts it
// behind the agency's own reverse proxy.
func (s *server) handleProxy(w http.ResponseWriter, r *http.Request) {
	raw := r.URL.Query().Get("url")
	if raw == "" {
		http.Error(w, "missing url parameter", http.StatusBadRequest)
		return
	}
	target, err := url.Parse(raw)
	if err != nil || (target.Scheme != "http" && target.Scheme != "https") {
		http.Error(w, "url must be http(s)", http.StatusBadRequest)
		return
	}
	resp, err := http.Get(raw) //nolint:noctx // dev proxy; upstream timeout via client defaults
	if err != nil {
		http.Error(w, fmt.Sprintf("upstream fetch failed: %v", err), http.StatusBadGateway)
		return
	}
	defer resp.Body.Close()
	if ct := resp.Header.Get("Content-Type"); ct != "" {
		w.Header().Set("Content-Type", ct)
	}
	w.Header().Set("Access-Control-Allow-Origin", "*")
	w.WriteHeader(resp.StatusCode)
	io.Copy(w, resp.Body)
}

// Manifest pins the authoritative schedule artifact and the wasm
// artifact that validates it. `feed_version` comes from the artifact
// itself — the server asks the module to parse feed_info.txt rather
// than duplicating GTFS knowledge in the host.
type Manifest struct {
	FeedHash     string  `json:"feed_hash"` // "sha256:<hex>" of the GTFS zip
	FeedVersion  *string `json:"feed_version,omitempty"`
	WasmHash     string  `json:"wasm_hash"` // sha256 of the artifact every host runs
	GeneratedAt  int64   `json:"generated_at"`
	Publisher    *string `json:"publisher,omitempty"`
}

func (s *server) handleManifest(w http.ResponseWriter, r *http.Request) {
	manifest := Manifest{
		WasmHash:    s.wasm.Hash(),
		GeneratedAt: time.Now().Unix(),
	}
	if s.feeder != nil {
		feed, err := s.feeder.Load(r.Context(), s.wasm)
		if err != nil {
			http.Error(w, fmt.Sprintf("feed unavailable: %v", err), http.StatusInternalServerError)
			return
		}
		manifest.FeedHash = feed.hash
		if v := feed.Version(r.Context(), s.wasm); v != "" {
			manifest.FeedVersion = &v
		}
	}
	if s.publisher != "" {
		manifest.Publisher = &s.publisher
	}
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(manifest)
}

// handleFeed serves the configured GTFS artifact itself.
func (s *server) handleFeed(w http.ResponseWriter, r *http.Request) {
	if s.feeder == nil {
		http.Error(w, "no feed configured (start with -feed <gtfs.zip>)", http.StatusNotFound)
		return
	}
	feed, err := s.feeder.Load(r.Context(), s.wasm)
	if err != nil {
		http.Error(w, fmt.Sprintf("feed unavailable: %v", err), http.StatusInternalServerError)
		return
	}
	w.Header().Set("Content-Type", "application/zip")
	w.Write(feed.bytes)
}

// loadedFeed is one pinned state of the GTFS artifact.
type loadedFeed struct {
	hash   string // "sha256:<hex>" of the zip
	bytes  []byte
	feedID uint32 // the wasm registry id
}

func (l *loadedFeed) Hash() string  { return l.hash }
func (l *loadedFeed) Bytes() []byte { return l.bytes }

// Version asks the artifact to parse feed_info.txt. Empty when the
// feed has no version — the host never duplicates GTFS knowledge.
func (l *loadedFeed) Version(ctx context.Context, host *WasmHost) string {
	info, err := host.FeedInfo(ctx, l.feedID)
	if err != nil || info == nil || info.FeedVersion == nil {
		return ""
	}
	return *info.FeedVersion
}

// fileFeed reads the GTFS artifact from disk, caching by mtime so a
// republished file is picked up without a restart (the "planner
// publishes, everyone re-pins" flow).
type fileFeed struct {
	path string

	mu     sync.Mutex
	mtime  time.Time
	loaded bool
	feed   *loadedFeed
}

// Load returns the current feed, reloading when the file changed.
func (f *fileFeed) Load(ctx context.Context, host *WasmHost) (*loadedFeed, error) {
	f.mu.Lock()
	defer f.mu.Unlock()

	info, err := os.Stat(f.path)
	if err != nil {
		return nil, err
	}
	if f.loaded && info.ModTime().Equal(f.mtime) {
		return f.feed, nil
	}

	body, err := os.ReadFile(f.path)
	if err != nil {
		return nil, err
	}
	sum := sha256.Sum256(body)

	// Register the fresh artifact in the module and drop the old one.
	feedID, err := host.LoadFeed(ctx, body)
	if err != nil {
		return nil, err
	}
	if f.loaded && f.feed != nil {
		host.DropFeed(ctx, f.feed.feedID)
	}
	f.loaded = true
	f.mtime = info.ModTime()
	f.feed = &loadedFeed{
		hash:   "sha256:" + hex.EncodeToString(sum[:]),
		bytes:  body,
		feedID: feedID,
	}
	return f.feed, nil
}
