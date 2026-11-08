package main

// The published network: a server started with -feed imports the zip
// once through the NETWORK CAPABILITY (cap-network — the module the
// browser runs) and serves the import's answer. Browsers register the
// network document; nobody downstream parses GTFS.

import (
	"bytes"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/hex"
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

const networkWasm = "../web/pkg/cap_network.wasm"

// served is GET /network's body: the import op's answer.
type served struct {
	Network     json.RawMessage `json:"network"`
	Boundary    json.RawMessage `json:"boundary"`
	Notes       json.RawMessage `json:"notes"`
	NetworkHash string          `json:"network_hash"`
}

func newNetworkHost(t *testing.T) *WasmHost {
	t.Helper()
	host, err := NewWasmHost(context.Background(), networkWasm)
	if err != nil {
		t.Fatalf("load network module: %v", err)
	}
	t.Cleanup(func() { host.Close(context.Background()) })
	return host
}

// networkServer serves /network and /manifest over the feed at path.
func networkServer(t *testing.T, path string) string {
	t.Helper()
	s := &server{wasm: newTestHost(t), feeder: fileFeed(path, networkWasm)}
	mux := http.NewServeMux()
	mux.HandleFunc("/network", s.handleNetwork)
	mux.HandleFunc("/network/document", s.handleNetworkDocument)
	mux.HandleFunc("/network/boundary", s.handleNetworkBoundary)
	mux.HandleFunc("/manifest", s.handleManifest)
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	return srv.URL
}

func writeFeed(t *testing.T, body []byte) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "feed.zip")
	if err := os.WriteFile(path, body, 0o644); err != nil {
		t.Fatal(err)
	}
	return path
}

func getNetwork(t *testing.T, base string) served {
	t.Helper()
	resp, err := http.Get(base + "/network")
	if err != nil {
		t.Fatalf("get /network: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != 200 {
		body, _ := io.ReadAll(resp.Body)
		t.Fatalf("/network: %d %s", resp.StatusCode, body)
	}
	var got served
	if err := json.NewDecoder(resp.Body).Decode(&got); err != nil {
		t.Fatalf("/network body: %v", err)
	}
	return got
}

// The served answer is the import's answer: the same network hash a
// module computes importing the same zip itself, and the served
// document registers under exactly that hash — the browser's check.
func TestServedNetworkIsTheImport(t *testing.T) {
	ctx := context.Background()
	zip := fixtureZip(t)
	got := getNetwork(t, networkServer(t, writeFeed(t, zip)))

	if !strings.HasPrefix(got.NetworkHash, "sha256:") || len(got.Network) == 0 || len(got.Boundary) == 0 {
		t.Fatalf("served answer incomplete: hash %q, network %d bytes, boundary %d bytes", got.NetworkHash, len(got.Network), len(got.Boundary))
	}

	// A module importing the zip itself (what a browser does with a
	// zip it is handed).
	own := newNetworkHost(t)
	var loaded struct {
		FeedHash string `json:"feed_hash"`
	}
	if err := own.InvokeBytes(ctx, opLoadFeed, zip, &loaded); err != nil {
		t.Fatalf("load_feed: %v", err)
	}
	var imported served
	if err := own.Invoke(ctx, map[string]any{"op": "import", "feed_hash": loaded.FeedHash}, &imported); err != nil {
		t.Fatalf("import: %v", err)
	}
	if imported.NetworkHash != got.NetworkHash {
		t.Fatalf("served hash %s != own import %s", got.NetworkHash, imported.NetworkHash)
	}

	// A fresh module registers the served document under the pin.
	fresh := newNetworkHost(t)
	var registered struct {
		NetworkHash string `json:"network_hash"`
	}
	if err := fresh.InvokeBytes(ctx, opLoadNetwork, got.Network, &registered); err != nil {
		t.Fatalf("load_network: %v", err)
	}
	if registered.NetworkHash != got.NetworkHash {
		t.Fatalf("served network registers as %s, pinned %s", registered.NetworkHash, got.NetworkHash)
	}
}

// The manifest pins what is served (network_hash) and what it was
// imported from (feed_hash, feed_version).
func TestManifestPinsTheServedNetwork(t *testing.T) {
	path := writeFeed(t, fixtureZip(t))
	base := networkServer(t, path)
	got := getNetwork(t, base)

	resp, err := http.Get(base + "/manifest")
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	var m Manifest
	if err := json.NewDecoder(resp.Body).Decode(&m); err != nil {
		t.Fatal(err)
	}
	if m.NetworkHash != got.NetworkHash {
		t.Fatalf("manifest network_hash %s != served %s", m.NetworkHash, got.NetworkHash)
	}
	if m.FeedHash != sha256File(t, path) {
		t.Fatalf("manifest feed_hash %s != zip %s", m.FeedHash, sha256File(t, path))
	}
	if m.FeedVersion == nil || *m.FeedVersion != "2026.10.1" {
		t.Fatalf("manifest feed_version %v", m.FeedVersion)
	}
}

// The manifest names the zone a page reads its clock in — the
// agency's, as the server counts its own today — so a page knows it at
// boot, before it picks the network in force on its date. The e2e
// fixture's agency is in America/Montreal.
func TestManifestNamesTheAgencysZone(t *testing.T) {
	base := networkServer(t, "../web/e2e/fixtures/sample-gtfs.zip")
	resp, err := http.Get(base + "/manifest")
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	var m Manifest
	if err := json.NewDecoder(resp.Body).Decode(&m); err != nil {
		t.Fatal(err)
	}
	if m.Timezone != "America/Montreal" {
		t.Fatalf("manifest timezone %q, want the fixture agency's America/Montreal", m.Timezone)
	}
}

// The wire: gzip when the client takes it, the network hash as the
// ETag, and a revalidation that matches answers 304 with no body.
func TestNetworkWire(t *testing.T) {
	base := networkServer(t, writeFeed(t, fixtureZip(t)))
	plain := getNetwork(t, base)

	req, _ := http.NewRequest("GET", base+"/network", nil)
	req.Header.Set("Accept-Encoding", "gzip")
	resp, err := http.DefaultTransport.RoundTrip(req)
	if err != nil {
		t.Fatal(err)
	}
	if resp.Header.Get("Content-Encoding") != "gzip" {
		t.Fatalf("Content-Encoding %q, want gzip", resp.Header.Get("Content-Encoding"))
	}
	etag := resp.Header.Get("ETag")
	if etag != `"`+plain.NetworkHash+`"` {
		t.Fatalf("ETag %s, want the network hash", etag)
	}
	zr, err := gzip.NewReader(resp.Body)
	if err != nil {
		t.Fatal(err)
	}
	unzipped, _ := io.ReadAll(zr)
	resp.Body.Close()
	var got served
	if err := json.Unmarshal(unzipped, &got); err != nil || got.NetworkHash != plain.NetworkHash || !bytes.Equal(got.Network, plain.Network) {
		t.Fatalf("gzip body is not the served answer (err %v)", err)
	}

	req, _ = http.NewRequest("GET", base+"/network", nil)
	req.Header.Set("If-None-Match", etag)
	resp, err = http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	body, _ := io.ReadAll(resp.Body)
	resp.Body.Close()
	if resp.StatusCode != http.StatusNotModified || len(body) != 0 {
		t.Fatalf("revalidation: %d with %d bytes, want 304 and none", resp.StatusCode, len(body))
	}
}

// Republishing the zip (planner publishes, everyone re-pins) serves
// the new network without a restart.
func TestRepublishedFeedServesTheNewNetwork(t *testing.T) {
	path := writeFeed(t, fixtureZip(t))
	base := networkServer(t, path)
	before := getNetwork(t, base)

	var buf bytes.Buffer
	files := map[string]string{
		"agency.txt":     "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n",
		"routes.txt":     "route_id,agency_id,route_short_name,route_type\n99,1,99,3\n",
		"stops.txt":      "stop_id,stop_name,stop_lat,stop_lon\nA,Main & 1st,45.500,-73.500\nB,Main & 2nd,45.510,-73.510\n",
		"trips.txt":      "route_id,service_id,trip_id\n99,WK,t99\n",
		"stop_times.txt": "trip_id,arrival_time,departure_time,stop_id,stop_sequence\nt99,08:00:00,08:00:00,A,1\nt99,08:10:00,08:10:00,B,2\n",
		"calendar.txt":   "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nWK,1,1,1,1,1,0,0,20260101,20261231\n",
	}
	if err := writeZip(&buf, files); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, buf.Bytes(), 0o644); err != nil {
		t.Fatal(err)
	}
	later := time.Now().Add(time.Minute)
	if err := os.Chtimes(path, later, later); err != nil {
		t.Fatal(err)
	}

	after := getNetwork(t, base)
	if after.NetworkHash == before.NetworkHash {
		t.Fatalf("republished feed still serves %s", before.NetworkHash)
	}
	if !strings.Contains(string(after.Network), `"99"`) {
		t.Fatalf("republished network lacks route 99")
	}
}

// The rider's wire: the bare network document gzipped, as bytes (no
// Content-Encoding — the browser hands them to the modules compressed
// and never unzips or parses them), under the network hash; its
// boundary beside it. The document is the import's own serialization
// (unzipped, it hashes to the pin), and a fresh module registers the
// compressed bytes under the pin.
func TestServedDocumentIsTheGzippedNetwork(t *testing.T) {
	ctx := context.Background()
	base := networkServer(t, writeFeed(t, fixtureZip(t)))
	envelope := getNetwork(t, base)

	resp, err := http.DefaultTransport.RoundTrip(mustRequest(t, base+"/network/document", "Accept-Encoding", "gzip"))
	if err != nil {
		t.Fatal(err)
	}
	zipped, _ := io.ReadAll(resp.Body)
	resp.Body.Close()
	if resp.StatusCode != 200 || resp.Header.Get("Content-Type") != "application/gzip" || resp.Header.Get("Content-Encoding") != "" {
		t.Fatalf("/network/document: %d, Content-Type %q, Content-Encoding %q", resp.StatusCode, resp.Header.Get("Content-Type"), resp.Header.Get("Content-Encoding"))
	}
	etag := resp.Header.Get("ETag")
	if etag != `"`+envelope.NetworkHash+`"` {
		t.Fatalf("ETag %s, want the network hash", etag)
	}
	zr, err := gzip.NewReader(bytes.NewReader(zipped))
	if err != nil {
		t.Fatal(err)
	}
	text, _ := io.ReadAll(zr)
	if !bytes.Equal(text, envelope.Network) {
		t.Fatal("the document is not the import's network")
	}
	if sum := sha256.Sum256(text); "sha256:"+hex.EncodeToString(sum[:]) != envelope.NetworkHash {
		t.Fatal("the document's bytes do not hash to the pin")
	}

	fresh := newNetworkHost(t)
	var registered struct {
		NetworkHash string `json:"network_hash"`
	}
	if err := fresh.InvokeBytes(ctx, opLoadNetwork, zipped, &registered); err != nil {
		t.Fatalf("load_network (gzip): %v", err)
	}
	if registered.NetworkHash != envelope.NetworkHash {
		t.Fatalf("the gzipped document registers as %s, pinned %s", registered.NetworkHash, envelope.NetworkHash)
	}

	resp, err = http.DefaultClient.Do(mustRequest(t, base+"/network/document", "If-None-Match", etag))
	if err != nil {
		t.Fatal(err)
	}
	body, _ := io.ReadAll(resp.Body)
	resp.Body.Close()
	if resp.StatusCode != http.StatusNotModified || len(body) != 0 {
		t.Fatalf("revalidation: %d with %d bytes, want 304 and none", resp.StatusCode, len(body))
	}

	resp, err = http.Get(base + "/network/boundary")
	if err != nil {
		t.Fatal(err)
	}
	boundary, _ := io.ReadAll(resp.Body)
	resp.Body.Close()
	if resp.StatusCode != 200 || resp.Header.Get("ETag") != etag || !bytes.Equal(boundary, envelope.Boundary) {
		t.Fatalf("/network/boundary: %d, ETag %q, body is the import's boundary: %v", resp.StatusCode, resp.Header.Get("ETag"), bytes.Equal(boundary, envelope.Boundary))
	}
}

func mustRequest(t *testing.T, url, header, value string) *http.Request {
	t.Helper()
	req, err := http.NewRequest("GET", url, nil)
	if err != nil {
		t.Fatal(err)
	}
	req.Header.Set(header, value)
	return req
}

// An open page revalidates the manifest on every poll (D-63): without
// the store too, the manifest answers an ETag naming what it pins — the
// same while nothing moved (generated_at aside), so a poll costs a 304.
func TestTheManifestRevalidatesWithoutTheStore(t *testing.T) {
	base := networkServer(t, "../web/e2e/fixtures/sample-gtfs.zip")
	get := func(etag string) *http.Response {
		req, _ := http.NewRequest(http.MethodGet, base+"/manifest", nil)
		if etag != "" {
			req.Header.Set("If-None-Match", etag)
		}
		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		resp.Body.Close()
		return resp
	}
	first := get("")
	etag := first.Header.Get("ETag")
	if first.StatusCode != 200 || etag == "" || first.Header.Get("Cache-Control") != "no-cache" {
		t.Fatalf("the manifest names its ETag and revalidates: %d %q %q", first.StatusCode, etag, first.Header.Get("Cache-Control"))
	}
	if again := get(""); again.Header.Get("ETag") != etag {
		t.Fatalf("nothing moved, the same ETag: %q vs %q", again.Header.Get("ETag"), etag)
	}
	if cached := get(etag); cached.StatusCode != http.StatusNotModified {
		t.Fatalf("an unchanged manifest answers 304: %d", cached.StatusCode)
	}
}
