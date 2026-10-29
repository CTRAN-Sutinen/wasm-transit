package main

// The published streets: a server started with -streets imports the
// street layer once through the STREETS CAPABILITY (cap-streets — the
// module the browser runs) and serves the canonical document; with a
// feed it serves the walk table over the published network too.
// Browsers register both by their bytes; nobody downstream reads the
// layer or computes the table.

import (
	"compress/gzip"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

const streetsWasm = "../web/pkg/cap_streets.wasm"

// The walking fixture: a residential street through the sample
// network's station bays, a motorway across it.
const walkStreetsLayer = "../web/e2e/fixtures/walk-streets.geojson"

func writeLayer(t *testing.T) string {
	t.Helper()
	body, err := os.ReadFile(walkStreetsLayer)
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "streets.geojson")
	if err := os.WriteFile(path, body, 0o644); err != nil {
		t.Fatal(err)
	}
	return path
}

// streetsServer serves /streets, /walks, /network and /manifest; feed
// "" publishes no network.
func streetsServer(t *testing.T, feed, layer string) string {
	t.Helper()
	s := &server{wasm: newTestHost(t), streets: &fileStreets{path: layer, streetsWasm: streetsWasm}}
	if feed != "" {
		s.feeder = &fileFeed{path: feed, networkWasm: networkWasm}
	}
	mux := http.NewServeMux()
	mux.HandleFunc("/network", s.handleNetwork)
	mux.HandleFunc("/streets", s.handleStreets)
	mux.HandleFunc("/walks", s.handleWalks)
	mux.HandleFunc("/manifest", s.handleManifest)
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	return srv.URL
}

// get fetches a path, gzip asked and undone, with its ETag.
func get(t *testing.T, url string) (int, []byte, string) {
	t.Helper()
	req, _ := http.NewRequest("GET", url, nil)
	req.Header.Set("Accept-Encoding", "gzip")
	resp, err := http.DefaultTransport.RoundTrip(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	var body io.Reader = resp.Body
	if resp.Header.Get("Content-Encoding") == "gzip" {
		zr, err := gzip.NewReader(resp.Body)
		if err != nil {
			t.Fatal(err)
		}
		body = zr
	}
	b, _ := io.ReadAll(body)
	return resp.StatusCode, b, strings.Trim(resp.Header.Get("ETag"), `"`)
}

func newStreetsHost(t *testing.T) *WasmHost {
	t.Helper()
	host, err := NewWasmHost(context.Background(), streetsWasm)
	if err != nil {
		t.Fatalf("load streets module: %v", err)
	}
	t.Cleanup(func() { host.Close(context.Background()) })
	return host
}

// The served streets are the import's document: a fresh module
// registers the bytes (load_streets, bulk op 1) under the ETag's
// hash; the walk table registers in the router under its own; the
// manifest pins both.
func TestServedStreetsAndWalksRegisterUnderTheirPins(t *testing.T) {
	ctx := context.Background()
	// The e2e fixture feed: its station's bays sit on the walking
	// fixture's street.
	zip, err := os.ReadFile("../web/e2e/fixtures/sample-gtfs.zip")
	if err != nil {
		t.Fatal(err)
	}
	base := streetsServer(t, writeFeed(t, zip), writeLayer(t))

	code, doc, streetsHash := get(t, base+"/streets")
	if code != 200 || !strings.HasPrefix(streetsHash, "sha256:") {
		t.Fatalf("/streets: %d, ETag %q", code, streetsHash)
	}
	var registered struct {
		StreetsHash string `json:"streets_hash"`
	}
	if err := newStreetsHost(t).InvokeBytes(ctx, opLoadStreets, doc, &registered); err != nil {
		t.Fatalf("load_streets: %v", err)
	}
	if registered.StreetsHash != streetsHash {
		t.Fatalf("served streets register as %s, pinned %s", registered.StreetsHash, streetsHash)
	}

	code, table, walksHash := get(t, base+"/walks")
	if code != 200 {
		t.Fatalf("/walks: %d %s", code, table)
	}
	var walks struct {
		StreetsHash string `json:"streets_hash"`
		NetworkHash string `json:"network_hash"`
		Walks       []struct {
			From string `json:"from"`
			To   string `json:"to"`
		} `json:"walks"`
	}
	if err := json.Unmarshal(table, &walks); err != nil {
		t.Fatal(err)
	}
	_, _, networkHash := get(t, base+"/network")
	if walks.StreetsHash != streetsHash || walks.NetworkHash != networkHash || len(walks.Walks) != 6 {
		t.Fatalf("the walk table walks %s × %s (%d walks), want %s × %s (the bays and the station, 6)",
			walks.StreetsHash, walks.NetworkHash, len(walks.Walks), streetsHash, networkHash)
	}
	router, err := NewWasmHost(ctx, "../web/public/caps/"+routerModuleFile(t))
	if err != nil {
		t.Fatal(err)
	}
	defer router.Close(ctx)
	var loaded struct {
		WalksHash string `json:"walks_hash"`
	}
	if err := router.Invoke(ctx, map[string]any{"op": "load_walks", "walks": json.RawMessage(table)}, &loaded); err != nil {
		t.Fatalf("load_walks: %v", err)
	}
	if loaded.WalksHash != walksHash {
		t.Fatalf("served walk table registers as %s, pinned %s", loaded.WalksHash, walksHash)
	}

	_, body, _ := get(t, base+"/manifest")
	var m Manifest
	if err := json.Unmarshal(body, &m); err != nil {
		t.Fatal(err)
	}
	if m.StreetsHash != streetsHash || m.WalksHash != walksHash {
		t.Fatalf("manifest pins %s / %s, served %s / %s", m.StreetsHash, m.WalksHash, streetsHash, walksHash)
	}

	req, _ := http.NewRequest("GET", base+"/streets", nil)
	req.Header.Set("If-None-Match", `"`+streetsHash+`"`)
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusNotModified {
		t.Fatalf("revalidation: %d, want 304", resp.StatusCode)
	}
}

// routerModuleFile is the router's hash-named file from the
// capability manifest.
func routerModuleFile(t *testing.T) string {
	t.Helper()
	body, err := os.ReadFile("../web/public/caps/manifest.json")
	if err != nil {
		t.Fatal(err)
	}
	var m struct {
		Capabilities []struct {
			ID  string `json:"id"`
			URL string `json:"url"`
		} `json:"capabilities"`
	}
	if err := json.Unmarshal(body, &m); err != nil {
		t.Fatal(err)
	}
	for _, c := range m.Capabilities {
		if c.ID == "router" {
			return strings.TrimPrefix(c.URL, "caps/")
		}
	}
	t.Fatal("the manifest has no router")
	return ""
}

// With no network published, the streets still serve and there is no
// walk table; a republished layer serves the new document.
func TestStreetsWithoutANetworkAndRepublished(t *testing.T) {
	layer := writeLayer(t)
	base := streetsServer(t, "", layer)
	code, _, before := get(t, base+"/streets")
	if code != 200 {
		t.Fatalf("/streets: %d", code)
	}
	if code, _, _ := get(t, base+"/walks"); code != http.StatusNotFound {
		t.Fatalf("/walks with no network: %d, want 404", code)
	}

	one := `{"type":"FeatureCollection","features":[{"type":"Feature","properties":{"highway":"residential"},` +
		`"geometry":{"type":"LineString","coordinates":[[-73.6,45.6],[-73.6,45.601]]}}]}`
	if err := os.WriteFile(layer, []byte(one), 0o644); err != nil {
		t.Fatal(err)
	}
	later := time.Now().Add(time.Second)
	os.Chtimes(layer, later, later)
	_, _, after := get(t, base+"/streets")
	if after == before {
		t.Fatalf("the republished layer still serves %s", before)
	}
}
