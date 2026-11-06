package main

import (
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// The shell's caching: a file whose name carries its content hash —
// Vite's hashed build output under assets/, the hash-addressed modules
// under caps/ — is the same bytes forever, so it is served immutable;
// everything else (the pages, caps/manifest.json, the deployment's
// config) revalidates on every use, so an open page meets the new build.
func TestStaticCacheHeaders(t *testing.T) {
	dist := t.TempDir()
	hex := strings.Repeat("ab", 32)
	files := map[string]string{
		"index.html":                    "no-cache",
		"stop-display.html":             "no-cache",
		"live-feeds.json":               "no-cache",
		"caps/manifest.json":            "no-cache",
		"caps/board-" + hex + ".wasm":   "public, max-age=31536000, immutable",
		"assets/index-Dhex5knD.js":      "public, max-age=31536000, immutable",
		"assets/AlertList-CwxRTjpU.css": "public, max-age=31536000, immutable",
		"assets/plain.js":               "no-cache", // no hash in its name
		"caps/board-abc.wasm":           "no-cache", // not a content hash
	}
	for name := range files {
		p := filepath.Join(dist, name)
		os.MkdirAll(filepath.Dir(p), 0o755)
		if err := os.WriteFile(p, []byte("x"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	s := &server{dist: dist}
	for name, want := range files {
		w := httptest.NewRecorder()
		s.handleStatic(w, httptest.NewRequest("GET", "/"+name, nil))
		if w.Code != http.StatusOK {
			t.Fatalf("%s: %d", name, w.Code)
		}
		if got := w.Header().Get("Cache-Control"); got != want {
			t.Errorf("%s: Cache-Control %q, want %q", name, got, want)
		}
	}
	// The root is the picker page: no-cache.
	w := httptest.NewRecorder()
	s.handleStatic(w, httptest.NewRequest("GET", "/", nil))
	if got := w.Header().Get("Cache-Control"); got != "no-cache" {
		t.Errorf("/: Cache-Control %q, want no-cache", got)
	}
}
