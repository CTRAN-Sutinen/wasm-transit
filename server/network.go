package main

import (
	"archive/zip"
	"bytes"
	"compress/gzip"
	"context"
	"encoding/csv"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"rustwasm/server/store"
)

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

// The document is application/gzip, never Content-Encoding: the browser
// hands the compressed bytes to the modules (kernel op 256 decodes gzip).
func (s *server) handleNetworkDocument(w http.ResponseWriter, r *http.Request) {
	s.servePublished(w, r, "application/gzip", func(f *loadedFeed) []byte { return f.documentGzip })
}

func (s *server) handleNetworkBoundary(w http.ResponseWriter, r *http.Request) {
	s.servePublished(w, r, "application/json", func(f *loadedFeed) []byte { return f.boundary })
}

func (s *server) servePublished(w http.ResponseWriter, r *http.Request, contentType string, body func(*loadedFeed) []byte) {
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
	if r.Header.Get("If-None-Match") == etag {
		w.WriteHeader(http.StatusNotModified)
		return
	}
	w.Header().Set("Content-Type", contentType)
	w.Write(body(feed))
}

type loadedFeed struct {
	hash        string          // "sha256:<hex>" of the zip
	version     string          // feed_info.txt's feed_version, "" when absent
	networkHash string          // the import's network_hash
	network     json.RawMessage // the import's answer, served as-is
	networkGzip []byte
	// The network's bytes hash to networkHash. The boundary is named by the
	// sha256 of its bytes (boundaryHash).
	document     json.RawMessage
	documentGzip []byte
	boundary     json.RawMessage
	boundaryGzip []byte
	boundaryHash string
	timezone     string
}

// importFeed closes its instance after the import, so the import's working
// memory (several times the network's size) goes with it.
func importFeed(ctx context.Context, wasmPath string, zipBytes []byte) (json.RawMessage, string, string, error) {
	inst, err := NewWasmHost(ctx, wasmPath)
	if err != nil {
		return nil, "", "", fmt.Errorf("the server's module: %w", err)
	}
	defer inst.Close(ctx)
	host := inst.As("network")
	var loaded struct {
		FeedHash string `json:"feed_hash"`
	}
	if err := host.InvokeBytes(ctx, opLoadFeed, zipBytes, &loaded); err != nil {
		return nil, "", "", fmt.Errorf("load_feed: %w", err)
	}
	answer, err := host.InvokeRaw(ctx, map[string]any{"op": "import", "feed_hash": loaded.FeedHash})
	if err != nil {
		return nil, "", "", fmt.Errorf("import: %w", err)
	}
	var pin struct {
		NetworkHash string `json:"network_hash"`
	}
	if err := json.Unmarshal(answer, &pin); err != nil || pin.NetworkHash == "" {
		return nil, "", "", fmt.Errorf("import answered no network_hash")
	}
	return answer, pin.NetworkHash, "cap-network@" + host.Hash(), nil
}

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

// feedSource re-imports only when the source's bytes change, so a
// republished feed needs no restart. In store mode a boot whose zip the
// store already imported imports nothing.
type feedSource struct {
	src      *source
	wasm     string // the server's module, a fresh instance of which imports it
	onImport func(*loadedFeed)
	imports  int

	mu   sync.Mutex
	feed *loadedFeed
	// zone is read without the lock: a write that Load's onImport makes reads
	// it (writeDate).
	zone atomic.Value
}

func fileFeed(path, wasm string) *feedSource {
	return &feedSource{src: newSource(path, "", "", nil), wasm: wasm}
}

func (f *feedSource) Zip() ([]byte, error) {
	body, _, err := f.src.Read()
	return body, err
}

func (f *feedSource) Load() (*loadedFeed, error) {
	f.mu.Lock()
	defer f.mu.Unlock()

	body, hash, err := f.src.Read()
	if err != nil {
		return nil, err
	}
	if f.feed != nil && f.feed.hash == hash {
		return f.feed, nil
	}
	started := time.Now()
	network, networkHash, stored, err := f.derive(body, hash)
	if err != nil {
		return nil, err
	}
	var parts struct {
		Network  json.RawMessage `json:"network"`
		Boundary json.RawMessage `json:"boundary"`
		Timezone string          `json:"timezone"`
	}
	if err := json.Unmarshal(network, &parts); err != nil || len(parts.Network) == 0 {
		return nil, fmt.Errorf("import answered no network document")
	}
	f.feed = &loadedFeed{
		hash:         hash,
		version:      feedInfoVersion(body),
		networkHash:  networkHash,
		network:      network,
		networkGzip:  gzipped(network),
		document:     parts.Network,
		documentGzip: gzipped(parts.Network),
		boundary:     parts.Boundary,
		boundaryGzip: gzipped(parts.Boundary),
		boundaryHash: store.HashOf(parts.Boundary),
		timezone:     parts.Timezone,
	}
	f.zone.Store(parts.Timezone)
	how := "imported"
	if stored {
		how = "read from the store"
	}
	log.Printf("network %s for %s in %s: %s (%d KB, %d KB gzipped)",
		how, f.src.raw, time.Since(started).Round(time.Millisecond), networkHash, len(parts.Network)/1024, len(f.feed.documentGzip)/1024)
	if f.onImport != nil {
		f.onImport(f.feed)
	}
	return f.feed, nil
}

func (f *feedSource) derive(body []byte, hash string) (json.RawMessage, string, bool, error) {
	st := f.src.st
	if st != nil {
		if answer, docs, ok := storedDerivation(st, derivedNetworkPointer, []string{hash}, "network", "boundary"); ok {
			network, err := answerWith(answer, map[string][]byte{"network": docs["network"], "boundary": docs["boundary"]})
			if err == nil {
				return network, store.HashOf(docs["network"]), true, nil
			}
			log.Printf("%s: %v: importing again", derivedNetworkPointer, err)
		}
	}
	network, networkHash, module, err := importFeed(context.Background(), f.wasm, body)
	if err != nil {
		return nil, "", false, err
	}
	f.imports++
	if st != nil {
		var parts struct {
			Network  json.RawMessage `json:"network"`
			Boundary json.RawMessage `json:"boundary"`
		}
		answer, err := answerWithout(network, "network", "boundary")
		if err == nil {
			err = json.Unmarshal(network, &parts)
		}
		if err != nil || len(parts.Network) == 0 || len(parts.Boundary) == 0 {
			log.Printf("%s: the import is not stored: %v", derivedNetworkPointer, err)
		} else {
			commitDerivation(st, derivedNetworkPointer, []string{hash}, module, answer,
				derivedDoc{tag: "network", storeKind: "network_document", version: networkVersion, body: parts.Network, hash: networkHash},
				derivedDoc{tag: "boundary", storeKind: "boundary", version: 1, body: parts.Boundary, hash: store.HashOf(parts.Boundary)})
		}
	}
	return network, networkHash, false, nil
}

func gzipped(b []byte) []byte {
	var gz bytes.Buffer
	zw, _ := gzip.NewWriterLevel(&gz, gzip.BestCompression)
	zw.Write(b)
	zw.Close()
	return gz.Bytes()
}
