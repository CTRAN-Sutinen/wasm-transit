package main

// The static bundle (D-131; docs/architecture/static-deployment.md):
// `-export DIR` writes files that the server serves at the same paths.
// These tests pin the layout both share:
//
//   - every published document is `blobs/<hash>` and `blobs/<hash>.gz`;
//   - the manifest names a blob for every pin it carries;
//   - the bundle's files are byte-for-byte what the server serves.

import (
	"bytes"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"io"
	"io/fs"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func exportServer(t *testing.T) *server {
	t.Helper()
	zip, err := os.ReadFile("../web/e2e/fixtures/sample-gtfs.zip")
	if err != nil {
		t.Fatal(err)
	}
	dist := t.TempDir()
	for name, body := range map[string]string{
		"index.html":               "<!doctype html>",
		"ride-now.html":            "<!doctype html><title>ride now</title>",
		"assets/main-AbCd1234.js":  "console.log(1)",
		"caps/manifest.json":       `{"kernel":1}`,
		"live-feeds.json":          `{"proxy":"/rt","trips":"http://example.test/tu"}`,
		"basemap-assets/glyph.pbf": "glyph",
	} {
		path := filepath.Join(dist, name)
		os.MkdirAll(filepath.Dir(path), 0o755)
		if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	return &server{
		dist:      dist,
		wasm:      newTestHost(t),
		feeder:    fileFeed(writeFeed(t, zip), networkWasm),
		streets:   fileStreets(writeLayer(t), streetsWasm),
		publisher: "Test Transit",
	}
}

func sha256Of(b []byte) string {
	sum := sha256.Sum256(b)
	return "sha256:" + hex.EncodeToString(sum[:])
}

func gunzip(t *testing.T, b []byte) []byte {
	t.Helper()
	zr, err := gzip.NewReader(bytes.NewReader(b))
	if err != nil {
		t.Fatalf("not gzip: %v", err)
	}
	out, err := io.ReadAll(zr)
	if err != nil {
		t.Fatal(err)
	}
	return out
}

func fetch(t *testing.T, url string) (*http.Response, []byte) {
	t.Helper()
	req, _ := http.NewRequest("GET", url, nil)
	resp, err := http.DefaultTransport.RoundTrip(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(resp.Body)
	return resp, body
}

func TestTheExportIsWhatTheServerServes(t *testing.T) {
	ctx := context.Background()
	s := exportServer(t)
	bundle := filepath.Join(t.TempDir(), "bundle")
	if err := s.export(bundle, ""); err != nil {
		t.Fatalf("export: %v", err)
	}
	srv := httptest.NewServer(s.routes())
	t.Cleanup(srv.Close)

	raw, err := os.ReadFile(filepath.Join(bundle, "manifest"))
	if err != nil {
		t.Fatalf("bundle manifest: %v", err)
	}
	var exported, live Manifest
	if err := json.Unmarshal(raw, &exported); err != nil {
		t.Fatal(err)
	}
	resp, body := fetch(t, srv.URL+"/manifest")
	if resp.StatusCode != 200 {
		t.Fatalf("/manifest: %d", resp.StatusCode)
	}
	json.Unmarshal(body, &live)
	exported.GeneratedAt, live.GeneratedAt = 0, 0
	if manifestKeyOf(exported) != manifestKeyOf(live) || exported.BoundaryHash != live.BoundaryHash {
		t.Fatalf("exported manifest %+v != served %+v", exported, live)
	}
	pins := map[string]string{
		"network": exported.NetworkHash, "boundary": exported.BoundaryHash,
		"streets": exported.StreetsHash, "walks": exported.WalksHash,
	}
	for name, hash := range pins {
		if !strings.HasPrefix(hash, "sha256:") {
			t.Fatalf("manifest pins no %s: %q", name, hash)
		}
	}

	for name, hash := range pins {
		plain, err := os.ReadFile(filepath.Join(bundle, "blobs", hash))
		if err != nil {
			t.Fatalf("%s blob: %v", name, err)
		}
		if got := sha256Of(plain); got != hash {
			t.Fatalf("%s blob hashes to %s, named %s", name, got, hash)
		}
		gz, err := os.ReadFile(filepath.Join(bundle, "blobs", hash+".gz"))
		if err != nil {
			t.Fatalf("%s .gz blob: %v", name, err)
		}
		if !bytes.Equal(gunzip(t, gz), plain) {
			t.Fatalf("%s .gz is not its blob gzipped", name)
		}
		resp, served := fetch(t, srv.URL+"/blobs/"+hash)
		if resp.StatusCode != 200 || !bytes.Equal(served, plain) {
			t.Fatalf("server /blobs/%s (%s): %d, %d bytes, want the bundle's %d", hash, name, resp.StatusCode, len(served), len(plain))
		}
		if !strings.Contains(resp.Header.Get("Cache-Control"), "immutable") {
			t.Fatalf("/blobs/%s not immutable: %q", hash, resp.Header.Get("Cache-Control"))
		}
		resp, served = fetch(t, srv.URL+"/blobs/"+hash+".gz")
		if resp.StatusCode != 200 || !bytes.Equal(served, gz) {
			t.Fatalf("server /blobs/%s.gz (%s): %d, not the bundle's bytes", hash, name, resp.StatusCode)
		}
		if resp.Header.Get("Content-Type") != "application/gzip" || resp.Header.Get("Content-Encoding") != "" {
			t.Fatalf("/blobs/%s.gz: Content-Type %q, Content-Encoding %q — the page hands the compressed bytes to the modules",
				hash, resp.Header.Get("Content-Type"), resp.Header.Get("Content-Encoding"))
		}
	}

	gzNetwork, _ := os.ReadFile(filepath.Join(bundle, "blobs", exported.NetworkHash+".gz"))
	var network struct {
		NetworkHash string `json:"network_hash"`
	}
	if err := newNetworkHost(t).InvokeBytes(ctx, opLoadNetwork, gzNetwork, &network); err != nil || network.NetworkHash != exported.NetworkHash {
		t.Fatalf("network blob registers as %q (%v), pinned %s", network.NetworkHash, err, exported.NetworkHash)
	}
	gzStreets, _ := os.ReadFile(filepath.Join(bundle, "blobs", exported.StreetsHash+".gz"))
	var streets struct {
		StreetsHash string `json:"streets_hash"`
	}
	if err := newStreetsHost(t).InvokeBytes(ctx, opLoadStreets, gzStreets, &streets); err != nil || streets.StreetsHash != exported.StreetsHash {
		t.Fatalf("streets blob registers as %q (%v), pinned %s", streets.StreetsHash, err, exported.StreetsHash)
	}

	err = filepath.WalkDir(s.dist, func(path string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return err
		}
		rel, _ := filepath.Rel(s.dist, path)
		want, _ := os.ReadFile(path)
		got, err := os.ReadFile(filepath.Join(bundle, rel))
		if err != nil || !bytes.Equal(got, want) {
			t.Errorf("bundle %s differs from -dist (%v)", rel, err)
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
}

// -live-feeds replaces the dev copy that -dist carries, whose `proxy`
// names a relay that a static host lacks.
func TestTheExportTakesTheDeploymentsLiveFeeds(t *testing.T) {
	s := exportServer(t)
	feeds := filepath.Join(t.TempDir(), "live-feeds.json")
	want := []byte(`{"trips":"https://feeds.example.test/tu"}`)
	os.WriteFile(feeds, want, 0o644)
	bundle := filepath.Join(t.TempDir(), "bundle")
	if err := s.export(bundle, feeds); err != nil {
		t.Fatalf("export: %v", err)
	}
	got, _ := os.ReadFile(filepath.Join(bundle, "live-feeds.json"))
	if !bytes.Equal(got, want) {
		t.Fatalf("bundle live-feeds.json = %s, want %s", got, want)
	}
}

// A bundle directory that holds anything is refused: uploading keeps what
// earlier bundles wrote.
func TestTheExportRefusesANonEmptyDirectory(t *testing.T) {
	s := exportServer(t)
	bundle := t.TempDir()
	os.WriteFile(filepath.Join(bundle, "stale"), []byte("x"), 0o644)
	if err := s.export(bundle, ""); err == nil {
		t.Fatal("export into a non-empty directory succeeded")
	}
}

// The export names the feed URL, imports none, pins no network, and writes no
// network blob (D-132).
func TestTheExportNamesAFeedURLAndImportsNothing(t *testing.T) {
	s := exportServer(t)
	s.feeder, s.streets = nil, nil
	s.feedURL = "https://feeds.example.test/gtfs.zip"
	bundle := filepath.Join(t.TempDir(), "bundle")
	if err := s.export(bundle, ""); err != nil {
		t.Fatalf("export: %v", err)
	}
	raw, err := os.ReadFile(filepath.Join(bundle, "manifest"))
	if err != nil {
		t.Fatal(err)
	}
	var m Manifest
	if err := json.Unmarshal(raw, &m); err != nil {
		t.Fatal(err)
	}
	if m.FeedURL != s.feedURL || m.NetworkHash != "" || m.BoundaryHash != "" {
		t.Fatalf("manifest feed_url %q network %q boundary %q — want the URL and no network", m.FeedURL, m.NetworkHash, m.BoundaryHash)
	}
	blobs, _ := os.ReadDir(filepath.Join(bundle, "blobs"))
	if len(blobs) != 0 {
		t.Fatalf("%d blobs written, want none", len(blobs))
	}
}
