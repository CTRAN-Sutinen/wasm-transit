package imports

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

	"rustwasm/server/source"
	"rustwasm/server/store"
	"rustwasm/server/wasmhost"
)

func (p *Published) HandleNetwork(w http.ResponseWriter, r *http.Request) {
	if p.Feed == nil {
		http.Error(w, "no feed configured (start with -feed <gtfs.zip>)", http.StatusNotFound)
		return
	}
	feed, err := p.Feed.Load()
	if err != nil {
		http.Error(w, fmt.Sprintf("network unavailable: %v", err), http.StatusInternalServerError)
		return
	}
	etag := `"` + feed.NetworkHash + `"`
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
		w.Write(feed.NetworkGzip)
		return
	}
	w.Write(feed.Network)
}

// The document is application/gzip, never Content-Encoding: the browser
// hands the compressed bytes to the modules (kernel op 256 decodes gzip).
func (p *Published) HandleNetworkDocument(w http.ResponseWriter, r *http.Request) {
	p.servePublished(w, r, "application/gzip", func(f *Feed) []byte { return f.DocumentGzip })
}

func (p *Published) HandleNetworkBoundary(w http.ResponseWriter, r *http.Request) {
	p.servePublished(w, r, "application/json", func(f *Feed) []byte { return f.Boundary })
}

func (p *Published) servePublished(w http.ResponseWriter, r *http.Request, contentType string, body func(*Feed) []byte) {
	if p.Feed == nil {
		http.Error(w, "no feed configured (start with -feed <gtfs.zip>)", http.StatusNotFound)
		return
	}
	feed, err := p.Feed.Load()
	if err != nil {
		http.Error(w, fmt.Sprintf("network unavailable: %v", err), http.StatusInternalServerError)
		return
	}
	etag := `"` + feed.NetworkHash + `"`
	w.Header().Set("ETag", etag)
	w.Header().Set("Cache-Control", "no-cache")
	if r.Header.Get("If-None-Match") == etag {
		w.WriteHeader(http.StatusNotModified)
		return
	}
	w.Header().Set("Content-Type", contentType)
	w.Write(body(feed))
}

type Feed struct {
	Hash        string          // "sha256:<hex>" of the zip
	Version     string          // feed_info.txt's feed_version, "" when absent
	NetworkHash string          // the import's network_hash
	Network     json.RawMessage // the import's answer, served as-is
	NetworkGzip []byte
	// The network's bytes hash to networkHash. The boundary is named by the
	// sha256 of its bytes (boundaryHash).
	Document     json.RawMessage
	DocumentGzip []byte
	Boundary     json.RawMessage
	BoundaryGzip []byte
	BoundaryHash string
	Timezone     string
}

// ImportFeed closes its instance after the import, so the import's working
// memory (several times the network's size) goes with it.
func ImportFeed(ctx context.Context, wasmPath string, zipBytes []byte) (json.RawMessage, string, string, error) {
	inst, err := wasmhost.New(ctx, wasmPath)
	if err != nil {
		return nil, "", "", fmt.Errorf("the server's module: %w", err)
	}
	defer inst.Close(ctx)
	host := inst.As("network")
	var loaded struct {
		FeedHash string `json:"feed_hash"`
	}
	if err := host.InvokeBytes(ctx, wasmhost.OpLoadFeed, zipBytes, &loaded); err != nil {
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

func FeedInfoVersion(zipBytes []byte) string {
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

// NetworkVersion mirrors transit_network::SCHEMA_VERSION, the version an
// import writes. An older document comes forward through cap-migrate.
const NetworkVersion = 2

// FeedSource re-imports only when the source's bytes change, so a
// republished feed needs no restart. In store mode a boot whose zip the
// store already imported imports nothing.
type FeedSource struct {
	Src      *source.Source
	Wasm     string // the server's module, a fresh instance of which imports it
	OnImport func(*Feed)
	Imports  int

	mu   sync.Mutex
	feed *Feed
	// Zone is read without the lock: a write that Load's onImport makes reads
	// it (writeDate).
	Zone atomic.Value
}

func FileFeed(path, wasm string) *FeedSource {
	return &FeedSource{Src: source.New(path, "", "", nil), Wasm: wasm}
}

func (f *FeedSource) Zip() ([]byte, error) {
	body, _, err := f.Src.Read()
	return body, err
}

func (f *FeedSource) Load() (*Feed, error) {
	f.mu.Lock()
	defer f.mu.Unlock()

	body, hash, err := f.Src.Read()
	if err != nil {
		return nil, err
	}
	if f.feed != nil && f.feed.Hash == hash {
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
	f.feed = &Feed{
		Hash:         hash,
		Version:      FeedInfoVersion(body),
		NetworkHash:  networkHash,
		Network:      network,
		NetworkGzip:  Gzipped(network),
		Document:     parts.Network,
		DocumentGzip: Gzipped(parts.Network),
		Boundary:     parts.Boundary,
		BoundaryGzip: Gzipped(parts.Boundary),
		BoundaryHash: store.HashOf(parts.Boundary),
		Timezone:     parts.Timezone,
	}
	f.Zone.Store(parts.Timezone)
	how := "imported"
	if stored {
		how = "read from the store"
	}
	log.Printf("network %s for %s in %s: %s (%d KB, %d KB gzipped)",
		how, f.Src.Raw(), time.Since(started).Round(time.Millisecond), networkHash, len(parts.Network)/1024, len(f.feed.DocumentGzip)/1024)
	if f.OnImport != nil {
		f.OnImport(f.feed)
	}
	return f.feed, nil
}

func (f *FeedSource) derive(body []byte, hash string) (json.RawMessage, string, bool, error) {
	st := f.Src.Store()
	if st != nil {
		if answer, docs, ok := StoredDerivation(st, DerivedNetworkPointer, []string{hash}, "network", "boundary"); ok {
			network, err := answerWith(answer, map[string][]byte{"network": docs["network"], "boundary": docs["boundary"]})
			if err == nil {
				return network, store.HashOf(docs["network"]), true, nil
			}
			log.Printf("%s: %v: importing again", DerivedNetworkPointer, err)
		}
	}
	network, networkHash, module, err := ImportFeed(context.Background(), f.Wasm, body)
	if err != nil {
		return nil, "", false, err
	}
	f.Imports++
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
			log.Printf("%s: the import is not stored: %v", DerivedNetworkPointer, err)
		} else {
			CommitDerivation(st, DerivedNetworkPointer, []string{hash}, module, answer,
				derivedDoc{tag: "network", storeKind: "network_document", version: NetworkVersion, body: parts.Network, hash: networkHash},
				derivedDoc{tag: "boundary", storeKind: "boundary", version: 1, body: parts.Boundary, hash: store.HashOf(parts.Boundary)})
		}
	}
	return network, networkHash, false, nil
}

func Gzipped(b []byte) []byte {
	var gz bytes.Buffer
	zw, _ := gzip.NewWriterLevel(&gz, gzip.BestCompression)
	zw.Write(b)
	zw.Close()
	return gz.Bytes()
}
