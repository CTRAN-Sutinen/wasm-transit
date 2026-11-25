package main

import (
	"archive/zip"
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"io"
	"os"
	"testing"
)

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

func sha256File(t *testing.T, path string) string {
	t.Helper()
	body, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	sum := sha256.Sum256(body)
	return "sha256:" + hex.EncodeToString(sum[:])
}
