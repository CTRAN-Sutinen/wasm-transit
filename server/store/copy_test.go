package store

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// oldDir writes a store in the layout an older build kept on disk:
// blobs/<hex> with a <hex>.meta.json sidecar, pointers/<name>.log as
// JSONL, a lock file, a .tmp left by an interrupted write, and an
// orphan no record names. It answers each pointer's log.
func oldDir(t *testing.T) (string, map[string][]LogEntry) {
	t.Helper()
	dir := t.TempDir()
	os.MkdirAll(filepath.Join(dir, "blobs"), 0o755)
	os.MkdirAll(filepath.Join(dir, "pointers"), 0o755)
	os.WriteFile(filepath.Join(dir, "lock"), nil, 0o644)
	blob := func(data []byte, e Envelope) string {
		h := hashOf(data)
		hex := strings.TrimPrefix(h, "sha256:")
		os.WriteFile(filepath.Join(dir, "blobs", hex), data, 0o644)
		meta, _ := json.Marshal(e)
		os.WriteFile(filepath.Join(dir, "blobs", hex+".meta.json"), append(meta, '\n'), 0o644)
		return h
	}
	logs := map[string][]LogEntry{}
	commit := func(name string, data []byte, e Envelope, writer string, at int64) {
		to := blob(data, e)
		from := ""
		if n := len(logs[name]); n > 0 {
			from = logs[name][n-1].To
		}
		logs[name] = append(logs[name], LogEntry{Seq: int64(len(logs[name]) + 1), From: from, To: to,
			Kind: e.Kind, SchemaVersion: e.SchemaVersion, ProducedBy: e.ProducedBy, Writer: writer, AtMs: at})
	}
	commit("edits", []byte(`{"kind":"edit_records","schema_version":2,"records":[]}`), env("edit_records"), "alice", 1)
	commit("edits", []byte(`{"kind":"edit_records","schema_version":2,"records":[{"id":"e1"}]}`), env("edit_records"), "bob", 2)
	commit("plans", []byte(`{"schema_version":1,"plans":[]}`), env("plans"), "server", 3)
	commit("manifest", []byte(`{"m":1}`), Envelope{Kind: "manifest", SchemaVersion: 1, ProducedBy: "server"}, "server", 4)
	for name, entries := range logs {
		var lines []byte
		for _, e := range entries {
			line, _ := json.Marshal(e)
			lines = append(append(lines, line...), '\n')
		}
		os.WriteFile(filepath.Join(dir, "pointers", name+".log"), lines, 0o644)
	}
	blob([]byte(`["orphan"]`), env("edit_records"))
	os.WriteFile(filepath.Join(dir, "blobs", strings.Repeat("ab", 32)+".tmp"), []byte("torn"), 0o644)
	return dir, logs
}

// A store an older build wrote, copied into a bucket, verifies and
// answers every pointer's head, log, and blobs as the directory held
// them — so a server over it serves the same manifest, plan set, and
// edit document.
func TestCopyCarriesADirectoryStoreIntoABucket(t *testing.T) {
	eachBackend(t, func(t *testing.T, b Backend) {
		dir, logs := oldDir(t)
		r, err := CopyDir(context.Background(), dir, b)
		if err != nil {
			t.Fatalf("copy: %v", err)
		}
		if r.Records != 4 || r.Blobs != 4 || r.Dropped != 1 || r.Verified.Records != 4 {
			t.Fatalf("report: %+v", r)
		}
		s := New(b)
		for name, entries := range logs {
			got, found, err := s.Log(name, "")
			if err != nil || !found || len(got) != len(entries) {
				t.Fatalf("%s log: %v %v %d", name, err, found, len(got))
			}
			for i := range entries {
				if got[i] != entries[i] {
					t.Fatalf("%s record %d: %+v != %+v", name, i+1, got[i], entries[i])
				}
			}
			data, e, err := s.GetBlob(entries[len(entries)-1].To)
			want, _ := os.ReadFile(filepath.Join(dir, "blobs", strings.TrimPrefix(entries[len(entries)-1].To, "sha256:")))
			if err != nil || string(data) != string(want) || e.Kind != entries[len(entries)-1].Kind {
				t.Fatalf("%s head blob: %q %+v %v", name, data, e, err)
			}
		}
		// A second copy over the same bucket is a no-op, not a conflict.
		if _, err := CopyDir(context.Background(), dir, b); err != nil {
			t.Fatalf("a re-run copy: %v", err)
		}
	})
}

// A log a crash tore (the NUL-padded tail a host crash left) is
// refused, naming the line: the copy never guesses at history.
func TestCopyRefusesATornLog(t *testing.T) {
	dir, _ := oldDir(t)
	f, _ := os.OpenFile(filepath.Join(dir, "pointers", "edits.log"), os.O_APPEND|os.O_WRONLY, 0o644)
	f.Write([]byte("{\"seq\":3,\x00\x00\x00"))
	f.Close()
	_, err := CopyDir(context.Background(), dir, NewMemory())
	if err == nil || !strings.Contains(err.Error(), "edits.log line 3") {
		t.Fatalf("a torn log is refused naming its line: %v", err)
	}
}

// A bucket that already holds a different history is never overwritten.
func TestCopyRefusesABucketWithAnotherHistory(t *testing.T) {
	dir, _ := oldDir(t)
	b := NewMemory()
	if _, _, err := New(b).CAS("edits", "", []byte(`["other"]`), env("edit_records"), "", "w", 1); err != nil {
		t.Fatal(err)
	}
	if _, err := CopyDir(context.Background(), dir, b); err == nil || !strings.Contains(err.Error(), "another history") {
		t.Fatalf("copy over another history: %v", err)
	}
}
