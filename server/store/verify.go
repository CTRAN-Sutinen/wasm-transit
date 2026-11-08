package store

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"sort"
	"strings"
)

// VerifyReport is what a passing verify read.
type VerifyReport struct {
	Pointers int `json:"pointers"`
	Records  int `json:"records"`
	Blobs    int `json:"blobs"`   // named blobs, each read and hashed
	Orphans  int `json:"orphans"` // blobs no record names (`store gc`'s, past the grace)
	Trash    int `json:"trash"`   // gc/ objects a crashed GC pass left (the next pass settles them)
}

// VerifyError is the first break verify found, naming its object.
type VerifyError struct {
	Key    string
	Reason string
}

func (e *VerifyError) Error() string { return fmt.Sprintf("store verify: %s: %s", e.Key, e.Reason) }

func broken(key, format string, args ...any) error {
	return &VerifyError{Key: key, Reason: fmt.Sprintf(format, args...)}
}

// Verify reads a whole store without writing (`store verify`; Go, no
// wasm): every key under pointers/ is a record, each pointer's records
// run from seq 1 with no gap and chain (`from` is the prior `to`), and
// every blob a record names (and every file of a build one names —
// build.go) exists, hashes to its name, and carries its envelope. It
// answers the first break as a *VerifyError. The copy, the backup, and
// the restore run it.
func Verify(ctx context.Context, b Backend) (VerifyReport, error) {
	var r VerifyReport
	objs, err := b.List(ctx, "pointers/", "")
	if err != nil {
		return r, err
	}
	byName := map[string][]int64{}
	for _, o := range objs {
		name, seq, ok := recordSeq(o.Key)
		if !ok {
			return r, broken(o.Key, "not a record key (pointers/<name>/<20-digit seq>.json)")
		}
		byName[name] = append(byName[name], seq)
	}
	names := make([]string, 0, len(byName))
	for n := range byName {
		names = append(names, n)
	}
	sort.Strings(names)
	named := map[string]bool{}
	builds := map[string]string{} // a build's hash → the first record naming it
	for _, name := range names {
		r.Pointers++
		prev := ""
		for i, seq := range byName[name] { // listed ascending
			key := recordKey(name, seq)
			if seq != int64(i+1) {
				return r, broken(key, "a gap: pointer %s has no record %d", name, i+1)
			}
			raw, _, err := b.Get(ctx, key)
			if err != nil {
				return r, err
			}
			var e LogEntry
			if err := json.Unmarshal(raw, &e); err != nil {
				return r, broken(key, "the record is not JSON: %v", err)
			}
			if e.Seq != seq {
				return r, broken(key, "the record says seq %d", e.Seq)
			}
			if e.From != prev {
				return r, broken(key, "does not chain: from %q, the prior record's to %q", e.From, prev)
			}
			h, err := ParseHash(e.To)
			if err != nil {
				return r, broken(key, "to: %v", err)
			}
			named[h] = true
			if e.Kind == BuildKind {
				builds[e.To] = key
			}
			prev = e.To
			r.Records++
		}
	}
	// A build names its files (build.go): each must be there, as its
	// hash, with its envelope, like the build itself.
	buildFile := map[string]string{} // a file's hex → the build naming it
	buildHashes := make([]string, 0, len(builds))
	for to := range builds {
		buildHashes = append(buildHashes, to)
	}
	sort.Strings(buildHashes)
	for _, to := range buildHashes {
		refs, err := buildRefs(ctx, b, to)
		if errors.Is(err, ErrNotFound) {
			continue // the blob check below names the missing build
		} else if err != nil {
			return r, broken(builds[to], "%v", err)
		}
		for _, x := range refs {
			if !named[x] {
				named[x] = true
				buildFile[x] = to
			}
		}
	}
	hexes := make([]string, 0, len(named))
	for h := range named {
		hexes = append(hexes, h)
	}
	sort.Strings(hexes)
	for _, h := range hexes {
		key := blobKey(h)
		data, meta, err := b.Get(ctx, key)
		if errors.Is(err, ErrNotFound) {
			if by, ok := buildFile[h]; ok {
				return r, broken(key, "build %s names it, and the blob is missing", by)
			}
			return r, broken(key, "a record names it, and the blob is missing")
		} else if err != nil {
			return r, err
		}
		if got := hashOf(data); got != "sha256:"+h {
			return r, broken(key, "the bytes hash to %s", got)
		}
		var env Envelope
		if err := json.Unmarshal([]byte(meta[envelopeMeta]), &env); err != nil || env.Kind == "" || env.SchemaVersion == 0 {
			return r, broken(key, "no envelope (kind, schema_version) in its metadata")
		}
		r.Blobs++
	}
	blobs, err := b.List(ctx, "blobs/", "")
	if err != nil {
		return r, err
	}
	for _, o := range blobs {
		h := strings.TrimPrefix(o.Key, "blobs/")
		if _, err := ParseHash("sha256:" + h); err != nil {
			return r, broken(o.Key, "not a blob key (blobs/<sha256 hex>)")
		}
		if !named[h] {
			r.Orphans++
		}
	}
	trash, err := b.List(ctx, "gc/", "")
	if err != nil {
		return r, err
	}
	r.Trash = len(trash)
	return r, nil
}
