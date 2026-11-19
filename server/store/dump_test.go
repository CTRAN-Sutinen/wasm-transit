package store

// A dump writes every object of a store to a folder: each at its key,
// its exact bytes, a blob's envelope beside it; and it refuses a folder
// that is not empty.

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestADumpWritesEveryObjectAtItsKey(t *testing.T) {
	ctx := context.Background()
	b := NewMemory()
	s := New(b)
	hash, _, err := s.CAS("plans", "", []byte(`{"plans":[]}`), env("plans"), "", "w", 1)
	if err != nil {
		t.Fatal(err)
	}
	if err := s.AppendEntry("stops", 1, entry(1, "w")); err != nil {
		t.Fatal(err)
	}
	dir := filepath.Join(t.TempDir(), "bucket")
	r, err := Dump(ctx, b, dir)
	if err != nil || r.Objects != 3 {
		t.Fatalf("the dump: %+v %v", r, err)
	}
	got, err := os.ReadFile(filepath.Join(dir, "logs", "stops", "entries", "00000000000000000001.json"))
	if err != nil || string(got) != string(entry(1, "w")) {
		t.Fatalf("the entry, its exact bytes: %q %v", got, err)
	}
	hex := strings.TrimPrefix(hash, "sha256:")
	if got, _ := os.ReadFile(filepath.Join(dir, "blobs", hex)); string(got) != `{"plans":[]}` {
		t.Fatalf("the blob: %q", got)
	}
	var meta struct {
		Envelope Envelope `json:"envelope"`
	}
	raw, _ := os.ReadFile(filepath.Join(dir, "blobs", hex+".meta.json"))
	if err := json.Unmarshal(raw, &meta); err != nil || meta.Envelope.Kind != "plans" {
		t.Fatalf("the blob's envelope beside it: %s %v", raw, err)
	}
	if _, err := os.Stat(filepath.Join(dir, "logs", "stops", "entries", "00000000000000000001.json.meta.json")); err == nil {
		t.Fatal("an object with no metadata has no .meta.json")
	}
	if _, err := Dump(ctx, b, dir); err == nil || !strings.Contains(err.Error(), "not empty") {
		t.Fatalf("a folder that is not empty is refused: %v", err)
	}
}
