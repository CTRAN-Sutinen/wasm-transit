package main

// Test helpers: a minimal zip writer for fixtures and hashing.

import (
	"archive/zip"
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"io"
	"os"
	"testing"
)

// writeZip builds an in-memory GTFS-style zip from file contents.
func writeZip(buf *bytes.Buffer, files map[string]string) error {
	w := zip.NewWriter(buf)
	for name, body := range files {
		f, err := w.Create(name)
		if err != nil {
			return err
		}
		if _, err := io.WriteString(f, body); err != nil {
			return err
		}
	}
	return w.Close()
}

// sha256File hashes a file as "sha256:<hex>" — the manifest format.
func sha256File(t *testing.T, path string) string {
	t.Helper()
	body, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	sum := sha256.Sum256(body)
	return "sha256:" + hex.EncodeToString(sum[:])
}
