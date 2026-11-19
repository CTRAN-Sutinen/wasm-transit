package store

import (
	"bytes"
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
	// The append-only logs (log.go): each log's entries and checkpoints.
	Logs        int `json:"logs"`
	Entries     int `json:"entries"`
	Checkpoints int `json:"checkpoints"`
	// Folded is how many checkpoints verify compared with the fold of
	// the entries before them (VerifyFolding).
	Folded int `json:"folded"`
}

// A LogFolder is the module that owns a log's kinds: the fold of the
// entries after a checkpoint (nil: from the empty document), answered
// as the checkpoint at the last entry, in its exact bytes.
type LogFolder func(log string, checkpoint []byte, entries []StoredEntry) ([]byte, error)

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
// answers the first break as a *VerifyError. Each log under logs/ runs
// from entry 1 with no gap, each entry says its log's kind, its log,
// and the seq in its key, and each checkpoint does the same, at a seq
// whose entry exists (verifyLogs). Whether a checkpoint equals the fold
// is the module's question: VerifyFolding asks it. The copy, the
// backup, and the restore run it.
func Verify(ctx context.Context, b Backend) (VerifyReport, error) {
	return VerifyFolding(ctx, b, nil)
}

// VerifyFolding is Verify, and with fold, each checkpoint is compared
// with the fold of the entries after the checkpoint before it (or from
// the first entry). A checkpoint that differs is a break.
func VerifyFolding(ctx context.Context, b Backend, fold LogFolder) (VerifyReport, error) {
	var r VerifyReport
	if err := verifyLogs(ctx, b, &r, fold); err != nil {
		return r, err
	}
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

// logHeader is the part of an entry or a checkpoint that verify reads:
// what it says it is, and where.
type logHeader struct {
	Kind string `json:"kind"`
	Log  string `json:"log"`
	Seq  int64  `json:"seq"`
}

// verifyLogs checks every log under logs/: the keys, the entries from 1
// with no gap, and each object's header against its key.
func verifyLogs(ctx context.Context, b Backend, r *VerifyReport, fold LogFolder) error {
	objs, err := b.List(ctx, "logs/", "")
	if err != nil {
		return err
	}
	type log struct{ entries, checkpoints []int64 }
	logs := map[string]*log{}
	var names []string
	for _, o := range objs {
		rest := strings.TrimPrefix(o.Key, "logs/")
		name, _, _ := strings.Cut(rest, "/")
		if !ValidPointerName(name) {
			return broken(o.Key, "not a log key (logs/<log>/entries|checkpoints/<20-digit seq>.json)")
		}
		l, ok := logs[name]
		if !ok {
			l = &log{}
			logs[name] = l
			names = append(names, name)
		}
		if seq, ok := keySeq(entriesPrefix(name), o.Key); ok {
			l.entries = append(l.entries, seq)
		} else if seq, ok := keySeq(checkpointsPrefix(name), o.Key); ok {
			l.checkpoints = append(l.checkpoints, seq)
		} else {
			return broken(o.Key, "not a log key (logs/<log>/entries|checkpoints/<20-digit seq>.json)")
		}
	}
	sort.Strings(names)
	// A log's entries are its own kind (the disruptions log's
	// disruptions_entry) or cap-log's (log_entry, D-197); so are its
	// checkpoints.
	read := func(key, name string, seq int64, kind string) error {
		raw, _, err := b.Get(ctx, key)
		if err != nil {
			return err
		}
		var h logHeader
		if err := json.Unmarshal(raw, &h); err != nil {
			return broken(key, "not JSON: %v", err)
		}
		generic := strings.Replace(kind, name+"_", "log_", 1)
		if (h.Kind != kind && h.Kind != generic) || h.Log != name || h.Seq != seq {
			return broken(key, "says kind %q, log %q, seq %d; its key says %s, %s, %d", h.Kind, h.Log, h.Seq, kind, name, seq)
		}
		return nil
	}
	for _, name := range names {
		l := logs[name]
		r.Logs++
		for i, seq := range l.entries { // listed ascending
			key := entryKey(name, seq)
			if seq != int64(i+1) {
				return broken(key, "a gap: log %s has no entry %d", name, i+1)
			}
			if err := read(key, name, seq, name+"_entry"); err != nil {
				return err
			}
			r.Entries++
		}
		var prev []byte
		var prevSeq int64
		for _, seq := range l.checkpoints {
			key := checkpointKey(name, seq)
			if seq > int64(len(l.entries)) {
				return broken(key, "a checkpoint past the log's last entry %d", len(l.entries))
			}
			if err := read(key, name, seq, name+"_checkpoint"); err != nil {
				return err
			}
			r.Checkpoints++
			if fold == nil {
				continue
			}
			stored, _, err := b.Get(ctx, key)
			if err != nil {
				return err
			}
			entries := make([]StoredEntry, 0, seq-prevSeq)
			for e := prevSeq + 1; e <= seq; e++ {
				data, _, err := b.Get(ctx, entryKey(name, e))
				if err != nil {
					return err
				}
				entries = append(entries, StoredEntry{Seq: e, Text: string(data)})
			}
			got, err := fold(name, prev, entries)
			if err != nil {
				return broken(key, "the fold of entries %d..%d: %v", prevSeq+1, seq, err)
			}
			if !bytes.Equal(got, stored) {
				return broken(key, "differs from the fold of entries %d..%d", prevSeq+1, seq)
			}
			r.Folded++
			prev, prevSeq = stored, seq
		}
	}
	return nil
}
