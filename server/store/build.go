package store

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"sort"
)

// Builds (D-80; docs/FORMATS.md, "Builds"). GC (gc.go) and Verify
// (verify.go) treat every file a build document names as named by its
// record, so every build the log ever held stays whole and a rollback is
// a pointer move to bytes that still exist. A store with no description
// (describe.go) relies on this reading.

const (
	BuildKind     = "build"
	BuildFileKind = "build_file"
)

type BuildDoc struct {
	Kind          string        `json:"kind"`
	SchemaVersion int           `json:"schema_version"`
	Files         []BuildFile   `json:"files"`
	Server        []BuildModule `json:"server,omitempty"`
}

type BuildFile struct {
	Path string `json:"path"`
	Hash string `json:"hash"`
	Type string `json:"type"`
}

type BuildModule struct {
	Role string `json:"role"`
	Hash string `json:"hash"`
}

// Sorted orders files by path and modules by role, so one tree is one
// document and one hash.
func (d BuildDoc) Sorted() BuildDoc {
	sort.Slice(d.Files, func(i, j int) bool { return d.Files[i].Path < d.Files[j].Path })
	sort.Slice(d.Server, func(i, j int) bool { return d.Server[i].Role < d.Server[j].Role })
	return d
}

func (d BuildDoc) Refs() []string {
	refs := make([]string, 0, len(d.Files)+len(d.Server))
	for _, f := range d.Files {
		refs = append(refs, f.Hash)
	}
	for _, m := range d.Server {
		refs = append(refs, m.Hash)
	}
	return refs
}

func ParseBuild(data []byte) (BuildDoc, error) {
	var d BuildDoc
	if err := json.Unmarshal(data, &d); err != nil {
		return d, fmt.Errorf("build document: %w", err)
	}
	if d.Kind != BuildKind || d.SchemaVersion != 1 {
		return d, fmt.Errorf("build document: kind %q version %d, want %q version 1", d.Kind, d.SchemaVersion, BuildKind)
	}
	for _, h := range d.Refs() {
		if _, err := ParseHash(h); err != nil {
			return d, fmt.Errorf("build document: %w", err)
		}
	}
	return d, nil
}

// CommitBuild writes every file first (files maps hash to bytes), then
// the document's CAS, then re-reads each file and re-creates it if a GC
// pass removed it between its create and the record (store.tla, Check).
// A document that names a file the call does not carry commits nothing.
func (s *Store) CommitBuild(name, from string, doc []byte, env Envelope, files map[string][]byte, fileEnv Envelope, writer string, atMs int64) (string, int64, error) {
	if env.Kind != BuildKind || fileEnv.Kind != BuildFileKind {
		return "", 0, fmt.Errorf("store: CommitBuild commits a %q of %q files, not a %q of %q", BuildKind, BuildFileKind, env.Kind, fileEnv.Kind)
	}
	d, err := ParseBuild(doc)
	if err != nil {
		return "", 0, err
	}
	refs := d.Refs()
	for _, h := range refs {
		if _, ok := files[h]; !ok {
			return "", 0, fmt.Errorf("store: the build names %s, and the commit does not carry it", h)
		}
	}
	for _, h := range refs {
		if _, err := s.putBlob(files[h], fileEnv, h); err != nil {
			return "", 0, err
		}
	}
	hash, seq, err := s.CAS(name, from, doc, env, "", writer, atMs)
	if err != nil {
		return "", 0, err
	}
	for _, h := range refs {
		x, _ := ParseHash(h)
		if _, err := s.b.Stat(context.Background(), blobKey(x)); errors.Is(err, ErrNotFound) {
			s.cache.drop(h)
			if _, err := s.putBlob(files[h], fileEnv, h); err != nil {
				return "", 0, fmt.Errorf("commit %s seq %d: re-create its file %s: %w", name, seq, h, err)
			}
		} else if err != nil {
			return "", 0, fmt.Errorf("commit %s seq %d: re-read its file %s: %w", name, seq, h, err)
		}
	}
	return hash, seq, nil
}

func buildRefs(ctx context.Context, b Backend, to string) ([]string, error) {
	h, err := ParseHash(to)
	if err != nil {
		return nil, err
	}
	data, _, err := b.Get(ctx, blobKey(h))
	if err != nil {
		return nil, fmt.Errorf("build %s: %w", to, err)
	}
	d, err := ParseBuild(data)
	if err != nil {
		return nil, err
	}
	out := []string{}
	for _, r := range d.Refs() {
		x, _ := ParseHash(r)
		out = append(out, x)
	}
	return out, nil
}
