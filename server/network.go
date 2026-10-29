package main

// The published schedule. The server is configured with the agency's
// GTFS zip (-feed) and publishes the NETWORK DOCUMENT: it imports the
// zip once through cap-network — the module the browser runs, so the
// same bytes everywhere — and serves the import's answer at
// GET /network. Browsers register the network; nobody downstream
// parses GTFS. The answer is opaque here: the host reads only its
// network_hash, to pin it.

import (
	"archive/zip"
	"bytes"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/csv"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"os"
	"strings"
	"sync"
	"time"
)

// handleNetwork serves the import's answer ({network, boundary,
// notes, network_hash}), gzipped when the client takes it, with the
// network hash as its ETag: a client holding the pinned document
// revalidates for a 304.
func (s *server) handleNetwork(w http.ResponseWriter, r *http.Request) {
	if s.feeder == nil {
		http.Error(w, "no feed configured (start with -feed <gtfs.zip>)", http.StatusNotFound)
		return
	}
	feed, err := s.feeder.Load()
	if err != nil {
		http.Error(w, fmt.Sprintf("network unavailable: %v", err), http.StatusInternalServerError)
		return
	}
	etag := `"` + feed.networkHash + `"`
	w.Header().Set("ETag", etag)
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Vary", "Accept-Encoding")
	if r.Header.Get("If-None-Match") == etag {
		w.WriteHeader(http.StatusNotModified)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	if strings.Contains(r.Header.Get("Accept-Encoding"), "gzip") {
		w.Header().Set("Content-Encoding", "gzip")
		w.Write(feed.networkGzip)
		return
	}
	w.Write(feed.network)
}

// loadedFeed is one published state: the zip (hashed, for provenance)
// and the network imported from it.
type loadedFeed struct {
	hash        string          // "sha256:<hex>" of the zip
	version     string          // feed_info.txt's feed_version, "" when absent
	networkHash string          // the import's network_hash
	network     json.RawMessage // the import's answer, served as-is
	networkGzip []byte
}

// importFeed runs load_feed + import in a fresh cap-network instance
// and closes it: the import's working memory (several times the
// network's size) goes with the instance.
func importFeed(ctx context.Context, wasmPath string, zipBytes []byte) (json.RawMessage, string, error) {
	host, err := NewWasmHost(ctx, wasmPath)
	if err != nil {
		return nil, "", fmt.Errorf("network module: %w", err)
	}
	defer host.Close(ctx)
	var loaded struct {
		FeedHash string `json:"feed_hash"`
	}
	if err := host.InvokeBytes(ctx, opLoadFeed, zipBytes, &loaded); err != nil {
		return nil, "", fmt.Errorf("load_feed: %w", err)
	}
	answer, err := host.InvokeRaw(ctx, map[string]any{"op": "import", "feed_hash": loaded.FeedHash})
	if err != nil {
		return nil, "", fmt.Errorf("import: %w", err)
	}
	var pin struct {
		NetworkHash string `json:"network_hash"`
	}
	if err := json.Unmarshal(answer, &pin); err != nil || pin.NetworkHash == "" {
		return nil, "", fmt.Errorf("import answered no network_hash")
	}
	return answer, pin.NetworkHash, nil
}

// feedInfoVersion pulls feed_version out of feed_info.txt in the
// zip — one metadata field for the manifest's provenance, the host's
// own I/O; the module owns the format.
func feedInfoVersion(zipBytes []byte) string {
	zr, err := zip.NewReader(bytes.NewReader(zipBytes), int64(len(zipBytes)))
	if err != nil {
		return ""
	}
	for _, f := range zr.File {
		if f.Name != "feed_info.txt" {
			continue
		}
		rc, err := f.Open()
		if err != nil {
			return ""
		}
		defer rc.Close()
		rows, err := csv.NewReader(rc).ReadAll()
		if err != nil || len(rows) < 2 {
			return ""
		}
		for i, col := range rows[0] {
			if col == "feed_version" && i < len(rows[1]) {
				return rows[1][i]
			}
		}
		return ""
	}
	return ""
}

// fileFeed reads the GTFS zip from disk and imports it, caching by
// mtime so a republished file is picked up without a restart (the
// "planner publishes, everyone re-pins" flow).
type fileFeed struct {
	path        string
	networkWasm string // the cap-network module that imports it

	mu     sync.Mutex
	mtime  time.Time
	loaded bool
	feed   *loadedFeed
}

// Load returns the current published state, re-importing when the
// file changed.
func (f *fileFeed) Load() (*loadedFeed, error) {
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
	started := time.Now()
	network, networkHash, err := importFeed(context.Background(), f.networkWasm, body)
	if err != nil {
		return nil, err
	}
	var gz bytes.Buffer
	zw, _ := gzip.NewWriterLevel(&gz, gzip.BestCompression)
	zw.Write(network)
	zw.Close()
	sum := sha256.Sum256(body)

	f.loaded = true
	f.mtime = info.ModTime()
	f.feed = &loadedFeed{
		hash:        "sha256:" + hex.EncodeToString(sum[:]),
		version:     feedInfoVersion(body),
		networkHash: networkHash,
		network:     network,
		networkGzip: gz.Bytes(),
	}
	log.Printf("network imported from %s in %s: %s (%d KB, %d KB gzipped)",
		f.path, time.Since(started).Round(time.Millisecond), networkHash, len(network)/1024, gz.Len()/1024)
	return f.feed, nil
}
