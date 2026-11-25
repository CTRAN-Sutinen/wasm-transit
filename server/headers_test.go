package main

// Pins the headers a deployment's pages must send (headers.go;
// docs/units/web-conformance.md, scope 2), in both shapes. The export
// writes the same rules as a `_headers` file.

import (
	"bytes"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func headersServer(t *testing.T) *server {
	t.Helper()
	dist := t.TempDir()
	big := strings.Repeat("console.log('a transit page');\n", 400)
	for name, body := range map[string]string{
		"index.html":               "<!doctype html>",
		"ride-now.html":            "<!doctype html><title>ride now</title>",
		"stop-display.html":        "<!doctype html><title>stop display</title>",
		"assets/main-AbCd1234.js":  big,
		"assets/main-AbCd1234.css": strings.Repeat("a{color:red}\n", 400),
		"caps/board-" + strings.Repeat("ab", 32) + ".wasm": "\x00asm" + strings.Repeat("\x01\x02\x03", 2000),
		"caps/manifest.json": `{"kernel":1}`,
		"basemap.pmtiles":    strings.Repeat("tiles", 2000),
		"live-feeds.json":    `{"proxy":"/rt","trips":"http://feeds.example.test/tu","alerts":"https://alerts.example.test:8443/a"}`,
	} {
		p := filepath.Join(dist, name)
		os.MkdirAll(filepath.Dir(p), 0o755)
		if err := os.WriteFile(p, []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	return &server{dist: dist}
}

func staticGet(s *server, path string, header map[string]string) *httptest.ResponseRecorder {
	r := httptest.NewRequest("GET", path, nil)
	for k, v := range header {
		r.Header.Set(k, v)
	}
	w := httptest.NewRecorder()
	s.handleStatic(w, r)
	return w
}

func TestStaticCompression(t *testing.T) {
	s := headersServer(t)
	hex := strings.Repeat("ab", 32)
	gz := map[string]string{"Accept-Encoding": "gzip, deflate, br"}

	for _, name := range []string{"assets/main-AbCd1234.js", "assets/main-AbCd1234.css", "caps/board-" + hex + ".wasm", "caps/manifest.json", "ride-now.html"} {
		want, _ := os.ReadFile(filepath.Join(s.dist, name))
		w := staticGet(s, "/"+name, gz)
		if w.Code != http.StatusOK {
			t.Fatalf("%s: %d", name, w.Code)
		}
		if w.Header().Get("Content-Encoding") != "gzip" {
			t.Errorf("%s: Content-Encoding %q, want gzip (type %s)", name, w.Header().Get("Content-Encoding"), w.Header().Get("Content-Type"))
			continue
		}
		if !strings.Contains(w.Header().Get("Vary"), "Accept-Encoding") {
			t.Errorf("%s: no Vary: Accept-Encoding", name)
		}
		if !bytes.Equal(gunzip(t, w.Body.Bytes()), want) {
			t.Errorf("%s: the gzip body does not decode to the file", name)
		}
		if w.Header().Get("Content-Length") != "" {
			t.Errorf("%s: Content-Length %q names the stored size", name, w.Header().Get("Content-Length"))
		}
		if w.Header().Get("Cache-Control") == "" {
			t.Errorf("%s: no Cache-Control", name)
		}
	}

	w := staticGet(s, "/assets/main-AbCd1234.js", nil)
	if w.Header().Get("Content-Encoding") != "" || !strings.HasPrefix(w.Body.String(), "console.log") {
		t.Errorf("no Accept-Encoding: got Content-Encoding %q", w.Header().Get("Content-Encoding"))
	}

	// A range is a range of the stored bytes.
	if w := staticGet(s, "/basemap.pmtiles", gz); w.Header().Get("Content-Encoding") != "" {
		t.Errorf("basemap.pmtiles: Content-Encoding %q", w.Header().Get("Content-Encoding"))
	}
	w = staticGet(s, "/assets/main-AbCd1234.js", map[string]string{"Accept-Encoding": "gzip", "Range": "bytes=0-9"})
	if w.Code != http.StatusPartialContent || w.Header().Get("Content-Encoding") != "" || w.Body.String() != "console.lo" {
		t.Errorf("range: %d, Content-Encoding %q, body %q", w.Code, w.Header().Get("Content-Encoding"), w.Body.String())
	}
}

// A strong ETag names one set of bytes, so the compressed bytes need
// another.
func TestGzipMakesTheETagWeak(t *testing.T) {
	handler := func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/javascript")
		w.Header().Set("ETag", `"sha256:abc"`)
		w.Write([]byte("x"))
	}
	r := httptest.NewRequest("GET", "/a.js", nil)
	r.Header.Set("Accept-Encoding", "gzip")
	w := httptest.NewRecorder()
	gzipStatic(w, r, handler)
	if got := w.Header().Get("ETag"); got != `W/"sha256:abc"` {
		t.Errorf("ETag %q, want a weak one", got)
	}
	w = httptest.NewRecorder()
	gzipStatic(w, r, func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/javascript")
		w.WriteHeader(http.StatusNotModified)
	})
	if w.Header().Get("Content-Encoding") != "" || w.Body.Len() != 0 {
		t.Errorf("304: Content-Encoding %q, %d body bytes", w.Header().Get("Content-Encoding"), w.Body.Len())
	}
}

func TestCompressibleTypes(t *testing.T) {
	for typ, want := range map[string]bool{
		"text/html; charset=utf-8":       true,
		"text/javascript; charset=utf-8": true,
		"text/css":                       true,
		"application/json":               true,
		"application/wasm":               true,
		"image/svg+xml":                  true,
		"application/gzip":               false, // the modules decode these documents
		"application/octet-stream":       false,
		"image/png":                      false,
		"font/woff2":                     false,
		"":                               false,
	} {
		if got := compressible(typ); got != want {
			t.Errorf("compressible(%q) = %v, want %v", typ, got, want)
		}
	}
}

func TestStaticSecurityHeaders(t *testing.T) {
	s := headersServer(t)
	for _, name := range []string{"/", "/index.html", "/ride-now.html", "/stop-display.html", "/assets/main-AbCd1234.js", "/live-feeds.json"} {
		w := staticGet(s, name, nil)
		if w.Header().Get("X-Content-Type-Options") != "nosniff" {
			t.Errorf("%s: X-Content-Type-Options %q", name, w.Header().Get("X-Content-Type-Options"))
		}
		if w.Header().Get("Referrer-Policy") != "strict-origin-when-cross-origin" {
			t.Errorf("%s: Referrer-Policy %q", name, w.Header().Get("Referrer-Policy"))
		}
	}

	// The picker and the forwarding index.html carry an inline script,
	// so they have no policy.
	for _, name := range []string{"/", "/index.html", "/assets/main-AbCd1234.js"} {
		if csp := staticGet(s, name, nil).Header().Get("Content-Security-Policy"); csp != "" {
			t.Errorf("%s: unexpected Content-Security-Policy %q", name, csp)
		}
	}

	ride := staticGet(s, "/ride-now.html", nil).Header()
	csp := ride.Get("Content-Security-Policy")
	for _, want := range []string{
		"default-src 'none'",
		"script-src 'self' 'wasm-unsafe-eval'",
		"worker-src 'self' blob:",
		"frame-ancestors 'none'",
		"object-src 'none'",
		// A relative proxy is the page's own origin.
		"http://feeds.example.test", "https://alerts.example.test:8443",
		"https://tiles.openfreemap.org", "https://protomaps.github.io",
	} {
		if !strings.Contains(csp, want) {
			t.Errorf("ride-now.html CSP lacks %q:\n%s", want, csp)
		}
	}
	if strings.Contains(csp, "unsafe-inline") || strings.Contains(csp, "'unsafe-eval'") {
		t.Errorf("ride-now.html CSP allows inline or eval code:\n%s", csp)
	}
	if got := ride.Get("Permissions-Policy"); !strings.Contains(got, "geolocation=(self)") || !strings.Contains(got, "camera=()") {
		t.Errorf("ride-now.html Permissions-Policy %q", got)
	}

	display := staticGet(s, "/stop-display.html", nil).Header()
	if csp := display.Get("Content-Security-Policy"); !strings.Contains(csp, "http://feeds.example.test") || strings.Contains(csp, "openfreemap") {
		t.Errorf("stop-display.html CSP: %s", csp)
	}
	if got := display.Get("Permissions-Policy"); !strings.Contains(got, "geolocation=()") {
		t.Errorf("stop-display.html Permissions-Policy %q (it allows no location)", got)
	}
}

func TestFeedOrigins(t *testing.T) {
	got := feedOrigins([]byte(`{"proxy":"/rt","trips":"http://a.test/x?y=1","vehicles":"https://b.test:9/v","alerts":"not a url","n":3}`), "https://agency.test/gtfs.zip")
	want := []string{"http://a.test", "https://agency.test", "https://b.test:9"}
	if strings.Join(got, " ") != strings.Join(want, " ") {
		t.Errorf("feedOrigins = %v, want %v", got, want)
	}
	if got := feedOrigins(nil, ""); len(got) != 0 {
		t.Errorf("no feeds: %v", got)
	}
	if got := feedOrigins([]byte("{broken"), ""); len(got) != 0 {
		t.Errorf("broken json: %v", got)
	}
}

// The export writes the caching of the hash-named files, the baseline,
// and a policy for each rider page the bundle holds.
func TestExportWritesTheHeadersFile(t *testing.T) {
	s := exportServer(t)
	dir := filepath.Join(t.TempDir(), "bundle")
	if err := s.export(dir, ""); err != nil {
		t.Fatal(err)
	}
	body, err := os.ReadFile(filepath.Join(dir, "_headers"))
	if err != nil {
		t.Fatalf("no _headers file: %v", err)
	}
	text := string(body)
	for _, want := range []string{
		"/*\n  X-Content-Type-Options: nosniff\n",
		"/assets/*\n  Cache-Control: " + immutableCache + "\n",
		"/caps/*\n  Cache-Control: " + immutableCache + "\n",
		"/blobs/*\n  Cache-Control: " + immutableCache + "\n",
		"/manifest\n  Content-Type: application/json\n",
		"/ride-now.html\n  Content-Security-Policy: default-src 'none'; script-src 'self' 'wasm-unsafe-eval'",
		"http://example.test",
		"Permissions-Policy: geolocation=(self)",
	} {
		if !strings.Contains(text, want) {
			t.Errorf("_headers lacks %q:\n%s", want, text)
		}
	}
	if strings.Contains(text, "/stop-display.html") {
		t.Errorf("_headers has a rule for a page the bundle does not hold:\n%s", text)
	}
	if strings.Contains(text, "/index.html") {
		t.Errorf("_headers has a policy for the forwarding index.html:\n%s", text)
	}
}

// A serverless deployment's policy names the agency's GTFS URL host.
func TestExportHeadersNameTheFeedURL(t *testing.T) {
	rules := deploymentRules([]string{"stop-display.html"}, feedOrigins(nil, "https://gtfs.agency.test/feed.zip"))
	text := renderHeadersFile(rules)
	if !strings.Contains(text, "connect-src 'self' https://gtfs.agency.test;") {
		t.Errorf("the policy does not name the feed URL's origin:\n%s", text)
	}
}
