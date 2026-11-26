package imports

// Browsers register the streets document by its bytes (the kernel's
// load_streets bulk op), and no browser reads the layer or computes the
// walk table. Both are opaque here: the host reads only their hashes.

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

	"rustwasm/server/source"
	"rustwasm/server/store"
	"rustwasm/server/wasmhost"
)

// cap-streets' own bulk op, apart from the kernel's wasmhost.OpLoadStreets.
const opImportStreets = 0

type Doc struct {
	Hash string
	Body []byte
	Gz   []byte
	Tag  string
}

func NewDoc(hash string, body []byte) Doc {
	var gz bytes.Buffer
	zw, _ := gzip.NewWriterLevel(&gz, gzip.BestCompression)
	zw.Write(body)
	zw.Close()
	return Doc{Hash: hash, Body: body, Gz: gz.Bytes()}
}

func (d Doc) serve(w http.ResponseWriter, r *http.Request) {
	etag := `"` + d.Hash + `"`
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
		w.Write(d.Gz)
		return
	}
	w.Write(d.Body)
}

// The walk table has an empty hash when no network is published.
type Streets struct {
	Streets Doc
	Walks   Doc
}

// In store mode a boot whose layer and network the store already
// derived from reads the stored documents and runs no module (derived.go).
type StreetsSource struct {
	Src  *source.Source
	Wasm string

	mu          sync.Mutex
	layerHash   string
	networkHash string
	loaded      *Streets
	Derivations int
}

func FileStreets(path, wasm string) *StreetsSource {
	return &StreetsSource{Src: source.New(path, "", "", nil), Wasm: wasm}
}

// A nil feed means no network and no walk table.
func (f *StreetsSource) Load(feed *Feed) (*Streets, error) {
	f.mu.Lock()
	defer f.mu.Unlock()

	body, layerHash, err := f.Src.Read()
	if err != nil {
		return nil, err
	}
	networkHash := ""
	if feed != nil {
		networkHash = feed.NetworkHash
	}
	if f.loaded != nil && layerHash == f.layerHash && networkHash == f.networkHash {
		return f.loaded, nil
	}

	ctx := context.Background()
	started := time.Now()
	st := f.Src.Store()
	var inst *wasmhost.Instance
	defer func() {
		if inst != nil {
			inst.Close(ctx)
		}
	}()
	open := func() (*wasmhost.Host, error) {
		if inst == nil {
			i, err := wasmhost.New(ctx, f.Wasm)
			if err != nil {
				return nil, fmt.Errorf("the server's module: %w", err)
			}
			inst = i
		}
		return inst.As("streets"), nil
	}
	streetsInHost := false

	var streets Doc
	if f.loaded != nil && layerHash == f.layerHash {
		streets = f.loaded.Streets // the same layer: the document already read
	} else if _, docs, ok := storedOf(st, DerivedStreetsPointer, []string{layerHash}, "streets"); ok {
		streets = NewDoc(store.HashOf(docs["streets"]), docs["streets"])
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
		streets = NewDoc(imported.StreetsHash, imported.Streets)
		streetsInHost = true
		f.Derivations++
		if st != nil {
			if answer, err := answerWithout(raw, "streets"); err != nil {
				log.Printf("%s: the import is not stored: %v", DerivedStreetsPointer, err)
			} else {
				CommitDerivation(st, DerivedStreetsPointer, []string{layerHash}, "cap-streets@"+h.Hash(), answer,
					derivedDoc{tag: "streets", storeKind: "streets", version: 1, body: streets.Body, hash: streets.Hash})
			}
		}
	}

	loaded := &Streets{Streets: streets}
	if feed != nil {
		inputs := []string{streets.Hash, feed.NetworkHash}
		if _, docs, ok := storedOf(st, DerivedWalksPointer, inputs, "walks"); ok {
			loaded.Walks = NewDoc(store.HashOf(docs["walks"]), docs["walks"])
		} else {
			h, err := open()
			if err != nil {
				return nil, err
			}
			if !streetsInHost {
				if err := h.InvokeBytes(ctx, wasmhost.OpLoadStreets, streets.Body, nil); err != nil {
					return nil, fmt.Errorf("load_streets: %w", err)
				}
			}
			if err := h.InvokeBytes(ctx, wasmhost.OpLoadNetwork, feed.Document, nil); err != nil {
				return nil, fmt.Errorf("load_network: %w", err)
			}
			raw, err := h.InvokeRaw(ctx, map[string]any{
				"op": "walks", "streets_hash": streets.Hash, "network_hash": feed.NetworkHash,
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
			loaded.Walks = NewDoc(walked.WalksHash, walked.Walks)
			f.Derivations++
			if st != nil {
				if answer, err := answerWithout(raw, "walks"); err != nil {
					log.Printf("%s: the walk table is not stored: %v", DerivedWalksPointer, err)
				} else {
					CommitDerivation(st, DerivedWalksPointer, inputs, "cap-streets@"+h.Hash(), answer,
						derivedDoc{tag: "walks", storeKind: "walks", version: 1, body: loaded.Walks.Body, hash: loaded.Walks.Hash})
				}
			}
		}
	}

	f.loaded = loaded
	f.layerHash = layerHash
	f.networkHash = networkHash
	how := "derived"
	if inst == nil {
		how = "read from the store"
	}
	log.Printf("streets %s for %s in %s: %s (%d KB, %d KB gzipped); walk table %s (%d KB gzipped)",
		how, f.Src.Raw(), time.Since(started).Round(time.Millisecond), streets.Hash, len(streets.Body)/1024, len(streets.Gz)/1024,
		loaded.Walks.Hash, len(loaded.Walks.Gz)/1024)
	return loaded, nil
}

func storedOf(st *store.Store, pointer string, inputs []string, tags ...string) (json.RawMessage, map[string][]byte, bool) {
	if st == nil {
		return nil, nil, false
	}
	return StoredDerivation(st, pointer, inputs, tags...)
}

func (p *Published) PublishedStreets() (*Streets, error) {
	var feed *Feed
	if p.Feed != nil {
		var err error
		if feed, err = p.Feed.Load(); err != nil {
			return nil, err
		}
	}
	return p.Streets.Load(feed)
}

func (p *Published) HandleStreets(w http.ResponseWriter, r *http.Request) {
	if p.Streets == nil {
		http.Error(w, "no streets configured (start with -streets <layer.geojson>)", http.StatusNotFound)
		return
	}
	loaded, err := p.PublishedStreets()
	if err != nil {
		http.Error(w, fmt.Sprintf("streets unavailable: %v", err), http.StatusInternalServerError)
		return
	}
	loaded.Streets.serve(w, r)
}

func (p *Published) HandleWalks(w http.ResponseWriter, r *http.Request) {
	if p.Streets == nil || p.Feed == nil {
		http.Error(w, "no walk table (start with -feed <gtfs.zip> and -streets <layer.geojson>)", http.StatusNotFound)
		return
	}
	loaded, err := p.PublishedStreets()
	if err != nil {
		http.Error(w, fmt.Sprintf("walk table unavailable: %v", err), http.StatusInternalServerError)
		return
	}
	loaded.Walks.serve(w, r)
}
