package main

// The published streets. A server started with -streets reads its
// street layer (GeoJSON lines: an agency export, TIGER, OSM) once
// through the STREETS CAPABILITY (cap-streets — the module the
// browser runs) and serves the canonical streets document at
// GET /streets; with a published network it also serves the WALK
// TABLE derived from the two at GET /walks. Browsers register the
// document by its bytes (the kernel's load_streets bulk op) and the
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
	"strings"
	"sync"
	"time"

	"rustwasm/server/store"
)

// cap-streets' own bulk op: import a street layer (a streets
// document registers on the kernel's opLoadStreets).
const opImportStreets = 0

// servedDoc is one published document: its bytes (and gzipped), under
// its hash.
type servedDoc struct {
	hash string
	body []byte
	gz   []byte
	// tag is the document's kind, as it names itself inside (its
	// schema at /schemas/). Set where the published documents are
	// listed.
	tag string
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

// streetsSource reads the street layer from its source (D-118) and
// imports it when its bytes change; the walk table is derived again
// when the published network changes. In store mode each is stored
// (derived.go): a boot whose layer and network the store already
// derived from reads the stored documents and runs no module.
type streetsSource struct {
	src         *source
	streetsWasm string // the cap-streets module that imports it

	mu          sync.Mutex
	layerHash   string
	networkHash string
	loaded      *loadedStreets
	// derivations counts the imports and walk tables this process made.
	derivations int
}

// fileStreets is a street layer read from a local file (dev, tests).
func fileStreets(path, streetsWasm string) *streetsSource {
	return &streetsSource{src: newSource(path, "", "", nil), streetsWasm: streetsWasm}
}

// Load returns the current published streets over `feed` (nil: no
// network, no walk table), importing the layer when the file changed
// and deriving the walk table when either changed.
func (f *streetsSource) Load(feed *loadedFeed) (*loadedStreets, error) {
	f.mu.Lock()
	defer f.mu.Unlock()

	body, layerHash, err := f.src.Read()
	if err != nil {
		return nil, err
	}
	networkHash := ""
	if feed != nil {
		networkHash = feed.networkHash
	}
	if f.loaded != nil && layerHash == f.layerHash && networkHash == f.networkHash {
		return f.loaded, nil
	}

	ctx := context.Background()
	started := time.Now()
	st := f.src.st
	// The module runs only when something is derived; its working memory
	// goes with the instance.
	var host *WasmHost
	defer func() {
		if host != nil {
			host.Close(ctx)
		}
	}()
	open := func() (*WasmHost, error) {
		if host == nil {
			h, err := NewWasmHost(ctx, f.streetsWasm)
			if err != nil {
				return nil, fmt.Errorf("streets module: %w", err)
			}
			host = h
		}
		return host, nil
	}
	streetsInHost := false

	var streets servedDoc
	if f.loaded != nil && layerHash == f.layerHash {
		streets = f.loaded.streets // the same layer: the document already read
	} else if _, docs, ok := storedOf(st, derivedStreetsPointer, []string{layerHash}, "streets"); ok {
		streets = newServedDoc(store.HashOf(docs["streets"]), docs["streets"])
	} else {
		h, err := open()
		if err != nil {
			return nil, err
		}
		var raw json.RawMessage
		if err := h.InvokeBytes(ctx, opImportStreets, body, &raw); err != nil {
			return nil, fmt.Errorf("import: %w", err)
		}
		var imported struct {
			StreetsHash string          `json:"streets_hash"`
			Streets     json.RawMessage `json:"streets"`
		}
		if err := json.Unmarshal(raw, &imported); err != nil {
			return nil, fmt.Errorf("import: %w", err)
		}
		streets = newServedDoc(imported.StreetsHash, imported.Streets)
		streetsInHost = true
		f.derivations++
		if st != nil {
			if answer, err := answerWithout(raw, "streets"); err != nil {
				log.Printf("%s: the import is not stored: %v", derivedStreetsPointer, err)
			} else {
				commitDerivation(st, derivedStreetsPointer, []string{layerHash}, "cap-streets@"+h.Hash(), answer,
					derivedDoc{tag: "streets", storeKind: "streets", version: 1, body: streets.body, hash: streets.hash})
			}
		}
	}

	loaded := &loadedStreets{streets: streets}
	if feed != nil {
		inputs := []string{streets.hash, feed.networkHash}
		if _, docs, ok := storedOf(st, derivedWalksPointer, inputs, "walks"); ok {
			loaded.walks = newServedDoc(store.HashOf(docs["walks"]), docs["walks"])
		} else {
			h, err := open()
			if err != nil {
				return nil, err
			}
			if !streetsInHost {
				if err := h.InvokeBytes(ctx, opLoadStreets, streets.body, nil); err != nil {
					return nil, fmt.Errorf("load_streets: %w", err)
				}
			}
			if err := h.InvokeBytes(ctx, opLoadNetwork, feed.document, nil); err != nil {
				return nil, fmt.Errorf("load_network: %w", err)
			}
			raw, err := h.InvokeRaw(ctx, map[string]any{
				"op": "walks", "streets_hash": streets.hash, "network_hash": feed.networkHash,
			})
			if err != nil {
				return nil, fmt.Errorf("walks: %w", err)
			}
			var walked struct {
				WalksHash string          `json:"walks_hash"`
				Walks     json.RawMessage `json:"walks"`
			}
			if err := json.Unmarshal(raw, &walked); err != nil {
				return nil, fmt.Errorf("walks: %w", err)
			}
			loaded.walks = newServedDoc(walked.WalksHash, walked.Walks)
			f.derivations++
			if st != nil {
				if answer, err := answerWithout(raw, "walks"); err != nil {
					log.Printf("%s: the walk table is not stored: %v", derivedWalksPointer, err)
				} else {
					commitDerivation(st, derivedWalksPointer, inputs, "cap-streets@"+h.Hash(), answer,
						derivedDoc{tag: "walks", storeKind: "walks", version: 1, body: loaded.walks.body, hash: loaded.walks.hash})
				}
			}
		}
	}

	f.loaded = loaded
	f.layerHash = layerHash
	f.networkHash = networkHash
	how := "derived"
	if host == nil {
		how = "read from the store"
	}
	log.Printf("streets %s for %s in %s: %s (%d KB, %d KB gzipped); walk table %s (%d KB gzipped)",
		how, f.src.raw, time.Since(started).Round(time.Millisecond), streets.hash, len(streets.body)/1024, len(streets.gz)/1024,
		loaded.walks.hash, len(loaded.walks.gz)/1024)
	return loaded, nil
}

// storedOf is storedDerivation, and false with no store.
func storedOf(st *store.Store, pointer string, inputs []string, tags ...string) (json.RawMessage, map[string][]byte, bool) {
	if st == nil {
		return nil, nil, false
	}
	return storedDerivation(st, pointer, inputs, tags...)
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
