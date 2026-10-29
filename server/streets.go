package main

// The published streets. A server started with -streets reads its
// street layer (GeoJSON lines: an agency export, TIGER, OSM) once
// through the STREETS CAPABILITY (cap-streets — the module the
// browser runs) and serves the canonical streets document at
// GET /streets; with a published network it also serves the WALK
// TABLE derived from the two at GET /walks. Browsers register the
// document by its bytes (cap-streets' load_streets bulk op) and the
// table in the router — nobody downstream reads the layer, and no
// browser computes the table. Both are opaque here: the host reads
// only their hashes, to pin them.

import (
	"bytes"
	"compress/gzip"
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"os"
	"strings"
	"sync"
	"time"
)

// cap-streets' bulk ops: a street layer, a streets document.
const (
	opImportStreets = 0
	opLoadStreets   = 1
)

// servedDoc is one published document: its bytes (and gzipped), under
// its hash.
type servedDoc struct {
	hash string
	body []byte
	gz   []byte
}

func newServedDoc(hash string, body []byte) servedDoc {
	var gz bytes.Buffer
	zw, _ := gzip.NewWriterLevel(&gz, gzip.BestCompression)
	zw.Write(body)
	zw.Close()
	return servedDoc{hash: hash, body: body, gz: gz.Bytes()}
}

// serve writes the document, gzipped when the client takes it, with
// its hash as the ETag: a client holding the pinned document
// revalidates for a 304.
func (d servedDoc) serve(w http.ResponseWriter, r *http.Request) {
	etag := `"` + d.hash + `"`
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
		w.Write(d.gz)
		return
	}
	w.Write(d.body)
}

// loadedStreets is one published state: the streets document, and
// the walk table over it and the published network (empty hash when
// no network is published).
type loadedStreets struct {
	streets servedDoc
	walks   servedDoc
}

// fileStreets reads the street layer from disk and imports it,
// caching by mtime; the walk table is derived again when the
// published network changes.
type fileStreets struct {
	path        string
	streetsWasm string // the cap-streets module that imports it

	mu          sync.Mutex
	mtime       time.Time
	networkHash string
	loaded      *loadedStreets
}

// Load returns the current published streets over `feed` (nil: no
// network, no walk table), importing the layer when the file changed
// and deriving the walk table when either changed.
func (f *fileStreets) Load(feed *loadedFeed) (*loadedStreets, error) {
	f.mu.Lock()
	defer f.mu.Unlock()

	info, err := os.Stat(f.path)
	if err != nil {
		return nil, err
	}
	networkHash := ""
	if feed != nil {
		networkHash = feed.networkHash
	}
	if f.loaded != nil && info.ModTime().Equal(f.mtime) && networkHash == f.networkHash {
		return f.loaded, nil
	}

	ctx := context.Background()
	started := time.Now()
	host, err := NewWasmHost(ctx, f.streetsWasm)
	if err != nil {
		return nil, fmt.Errorf("streets module: %w", err)
	}
	// The import's working memory goes with the instance.
	defer host.Close(ctx)

	var streets servedDoc
	if f.loaded != nil && info.ModTime().Equal(f.mtime) {
		// The same layer: register the document already imported.
		streets = f.loaded.streets
		if err := host.InvokeBytes(ctx, opLoadStreets, streets.body, nil); err != nil {
			return nil, fmt.Errorf("load_streets: %w", err)
		}
	} else {
		body, err := os.ReadFile(f.path)
		if err != nil {
			return nil, err
		}
		var imported struct {
			StreetsHash string          `json:"streets_hash"`
			Streets     json.RawMessage `json:"streets"`
		}
		if err := host.InvokeBytes(ctx, opImportStreets, body, &imported); err != nil {
			return nil, fmt.Errorf("import: %w", err)
		}
		streets = newServedDoc(imported.StreetsHash, imported.Streets)
	}

	loaded := &loadedStreets{streets: streets}
	if feed != nil {
		var answer struct {
			Network json.RawMessage `json:"network"`
		}
		if err := json.Unmarshal(feed.network, &answer); err != nil {
			return nil, fmt.Errorf("the published network: %w", err)
		}
		if err := host.InvokeBytes(ctx, opLoadNetwork, answer.Network, nil); err != nil {
			return nil, fmt.Errorf("load_network: %w", err)
		}
		var walked struct {
			WalksHash string          `json:"walks_hash"`
			Walks     json.RawMessage `json:"walks"`
		}
		err := host.Invoke(ctx, map[string]any{
			"op": "walks", "streets_hash": streets.hash, "network_hash": feed.networkHash,
		}, &walked)
		if err != nil {
			return nil, fmt.Errorf("walks: %w", err)
		}
		loaded.walks = newServedDoc(walked.WalksHash, walked.Walks)
	}

	f.loaded = loaded
	f.mtime = info.ModTime()
	f.networkHash = networkHash
	log.Printf("streets from %s in %s: %s (%d KB, %d KB gzipped); walk table %s (%d KB gzipped)",
		f.path, time.Since(started).Round(time.Millisecond), streets.hash, len(streets.body)/1024, len(streets.gz)/1024,
		loaded.walks.hash, len(loaded.walks.gz)/1024)
	return loaded, nil
}

// publishedStreets loads the streets over the published network (if
// any).
func (s *server) publishedStreets() (*loadedStreets, error) {
	var feed *loadedFeed
	if s.feeder != nil {
		var err error
		if feed, err = s.feeder.Load(); err != nil {
			return nil, err
		}
	}
	return s.streets.Load(feed)
}

// handleStreets serves the canonical streets document (bare — the
// bytes cap-streets' load_streets takes).
func (s *server) handleStreets(w http.ResponseWriter, r *http.Request) {
	if s.streets == nil {
		http.Error(w, "no streets configured (start with -streets <layer.geojson>)", http.StatusNotFound)
		return
	}
	loaded, err := s.publishedStreets()
	if err != nil {
		http.Error(w, fmt.Sprintf("streets unavailable: %v", err), http.StatusInternalServerError)
		return
	}
	loaded.streets.serve(w, r)
}

// handleWalks serves the walk table over the published streets and
// network.
func (s *server) handleWalks(w http.ResponseWriter, r *http.Request) {
	if s.streets == nil || s.feeder == nil {
		http.Error(w, "no walk table (start with -feed <gtfs.zip> and -streets <layer.geojson>)", http.StatusNotFound)
		return
	}
	loaded, err := s.publishedStreets()
	if err != nil {
		http.Error(w, fmt.Sprintf("walk table unavailable: %v", err), http.StatusInternalServerError)
		return
	}
	loaded.walks.serve(w, r)
}
