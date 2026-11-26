package main

import (
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// Pins that a file named by its content hash (assets/, caps/) is served
// immutable and everything else revalidates.
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
		s.static().HandleStatic(w, httptest.NewRequest("GET", "/"+name, nil))
		if w.Code != http.StatusOK {
			t.Fatalf("%s: %d", name, w.Code)
		}
		if got := w.Header().Get("Cache-Control"); got != want {
			t.Errorf("%s: Cache-Control %q, want %q", name, got, want)
		}
	}
	w := httptest.NewRecorder()
	s.static().HandleStatic(w, httptest.NewRequest("GET", "/", nil))
	if got := w.Header().Get("Cache-Control"); got != "no-cache" {
		t.Errorf("/: Cache-Control %q, want no-cache", got)
	}
}
