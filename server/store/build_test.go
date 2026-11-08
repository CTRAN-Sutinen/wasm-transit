package store

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"testing"
	"time"
)

// A build (D-80, docs/FORMATS.md "Builds") is the one kind whose blob
// names other blobs: every file the release built is a blob, and the
// build document lists them. Every build the `build` pointer ever
// logged stays whole — its document and every file it names — so a
// rollback is a pointer move to bytes that still exist.

var buildEnv = Envelope{Kind: BuildKind, SchemaVersion: 1, ProducedBy: "server"}
var fileEnv = Envelope{Kind: BuildFileKind, SchemaVersion: 1, ProducedBy: "server"}

// build makes a build document over the given files (path → bytes).
func build(t *testing.T, files map[string]string) ([]byte, map[string][]byte) {
	t.Helper()
	doc := BuildDoc{Kind: BuildKind, SchemaVersion: 1}
	bytes := map[string][]byte{}
	for path, body := range files {
		h := hashOf([]byte(body))
		doc.Files = append(doc.Files, BuildFile{Path: path, Hash: h, Type: "text/plain"})
		bytes[h] = []byte(body)
	}
	data, err := json.Marshal(doc.Sorted())
	if err != nil {
		t.Fatal(err)
	}
	return data, bytes
}

func aged(b Backend) *Store {
	gc := New(b)
	gc.now = func() time.Time { return time.Now().Add(OrphanGrace + time.Hour) }
	return gc
}

func TestEveryLoggedBuildKeepsItsFilesThroughGC(t *testing.T) {
	each(t, func(t *testing.T, b Backend, s *Store) {
		a, aFiles := build(t, map[string]string{"index.html": "A", "sw.js": "build A"})
		hashA, _, err := s.CommitBuild("build", "", a, buildEnv, aFiles, fileEnv, "import:dist", 1)
		if err != nil {
			t.Fatal(err)
		}
		bDoc, bFiles := build(t, map[string]string{"index.html": "B", "sw.js": "build B"})
		if _, _, err := s.CommitBuild("build", hashA, bDoc, buildEnv, bFiles, fileEnv, "import:dist", 2); err != nil {
			t.Fatal(err)
		}
		stray, err := s.PutBlob([]byte("no build names me"), fileEnv, "")
		if err != nil {
			t.Fatal(err)
		}
		r, err := aged(b).GC(OrphanGraceMs)
		if err != nil || r.Removed != 1 {
			t.Fatalf("one pass removes the stray file only: %+v %v", r, err)
		}
		if _, _, err := New(b).GetBlob(stray); !errors.Is(err, ErrNotFound) {
			t.Fatalf("a file no logged build names is an orphan: %v", err)
		}
		for h, want := range aFiles {
			if data, gotEnv, err := New(b).GetBlob(h); err != nil || string(data) != string(want) || gotEnv != fileEnv {
				t.Fatalf("build A is no longer the head and its files stay: %q %+v %v", data, gotEnv, err)
			}
		}
		if vr, err := Verify(context.Background(), b); err != nil || vr.Orphans != 0 {
			t.Fatalf("verify reads every build's files as named: %+v %v", vr, err)
		}
	})
}

func TestABuildCommitRecreatesAFileGCRemovedUnderIt(t *testing.T) {
	eachBackend(t, func(t *testing.T, b Backend) {
		ctx := context.Background()
		doc, files := build(t, map[string]string{"index.html": "reused"})
		var h string
		for k := range files {
			h, _ = ParseHash(k)
		}
		s := New(hooked{b, func(key string) {
			if strings.HasPrefix(key, "pointers/") {
				b.Delete(ctx, "blobs/"+h) // GC, between the record and the Check
			}
		}})
		if _, _, err := s.CommitBuild("build", "", doc, buildEnv, files, fileEnv, "w", 1); err != nil {
			t.Fatal(err)
		}
		if data, gotEnv, err := New(b).GetBlob("sha256:" + h); err != nil || string(data) != "reused" || gotEnv != fileEnv {
			t.Fatalf("the commit re-created the file with its envelope: %q %+v %v", data, gotEnv, err)
		}
	})
}

func TestVerifyFindsABuildsMissingFile(t *testing.T) {
	each(t, func(t *testing.T, b Backend, s *Store) {
		doc, files := build(t, map[string]string{"index.html": "A", "sw.js": "A"})
		if _, _, err := s.CommitBuild("build", "", doc, buildEnv, files, fileEnv, "w", 1); err != nil {
			t.Fatal(err)
		}
		h, _ := ParseHash(hashOf([]byte("A")))
		b.Delete(context.Background(), "blobs/"+h)
		_, err := Verify(context.Background(), b)
		var ve *VerifyError
		if !errors.As(err, &ve) || ve.Key != "blobs/"+h || !strings.Contains(ve.Reason, "build") {
			t.Fatalf("verify names the missing file and the build that names it: %v", err)
		}
	})
}

func TestABuildThatNamesAFileItDoesNotCarryIsRefused(t *testing.T) {
	each(t, func(t *testing.T, b Backend, s *Store) {
		doc, files := build(t, map[string]string{"index.html": "A", "sw.js": "A2"})
		for h := range files {
			delete(files, h)
			break
		}
		if _, _, err := s.CommitBuild("build", "", doc, buildEnv, files, fileEnv, "w", 1); err == nil {
			t.Fatal("a build missing one of its files commits nothing")
		}
		if _, found, _ := s.Head("build"); found {
			t.Fatal("no record landed")
		}
		if _, _, err := s.CommitBuild("build", "", doc, Envelope{Kind: "manifest", SchemaVersion: 1}, files, fileEnv, "w", 1); err == nil {
			t.Fatal("CommitBuild commits only the build kind")
		}
	})
}

func TestABuildsFilesAreListedInPathOrder(t *testing.T) {
	doc, _ := build(t, map[string]string{"sw.js": "1", "assets/a.js": "2", "index.html": "3"})
	var got BuildDoc
	if err := json.Unmarshal(doc, &got); err != nil {
		t.Fatal(err)
	}
	paths := []string{}
	for _, f := range got.Files {
		paths = append(paths, f.Path)
	}
	if strings.Join(paths, ",") != "assets/a.js,index.html,sw.js" {
		t.Fatalf("one tree is one document: %v", paths)
	}
}
