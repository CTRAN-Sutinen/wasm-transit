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

type VerifyReport struct {
	Pointers    int `json:"pointers"`
	Records     int `json:"records"`
	Blobs       int `json:"blobs"`
	Orphans     int `json:"orphans"`
	Trash       int `json:"trash"`
	Logs        int `json:"logs"`
	Entries     int `json:"entries"`
	Checkpoints int `json:"checkpoints"`
	Folded      int `json:"folded"`
	Schemas     int `json:"schemas"`
	Tables      int `json:"tables"`
	Referenced  int `json:"referenced"`
	// A broken reference is a finding for a person, not a break of the
	// store (D-233).
	References *ReferenceFindings `json:"references,omitempty"`
}

type ReferenceChecker func(heads map[string][]byte, entries map[string][]StoredEntry) (*ReferenceFindings, error)

// ReferenceFindings carries the kind table's hash, the module's hash, and
// the seq of each log it read, so a finding can be judged again after a
// release or a rollback.
type ReferenceFindings struct {
	KindTable string            `json:"kind_table"`
	Module    string            `json:"module"`
	Seqs      map[string]int64  `json:"seqs"`
	Broken    []json.RawMessage `json:"broken"`
	// Unjudged are records in a version newer than the kind table reads.
	// They are a warning.
	Unjudged []json.RawMessage `json:"unjudged,omitempty"`
}

type LogFolder func(log string, checkpoint []byte, entries []StoredEntry) ([]byte, error)

type VerifyError struct {
	Key    string
	Reason string
}

func (e *VerifyError) Error() string { return fmt.Sprintf("store verify: %s: %s", e.Key, e.Reason) }

func broken(key, format string, args ...any) error {
	return &VerifyError{Key: key, Reason: fmt.Sprintf(format, args...)}
}

// Whether a checkpoint equals the fold is the module's question, so
// VerifyFolding asks it.
func Verify(ctx context.Context, b Backend) (VerifyReport, error) {
	return VerifyFolding(ctx, b, nil)
}

func VerifyFolding(ctx context.Context, b Backend, fold LogFolder) (VerifyReport, error) {
	return VerifyChecking(ctx, b, fold, nil)
}

// refs needs fold (D-233).
func VerifyChecking(ctx context.Context, b Backend, fold LogFolder, refs ReferenceChecker) (VerifyReport, error) {
	var r VerifyReport
	if refs != nil && fold == nil {
		return r, fmt.Errorf("store verify: the existence check needs the log module to fold each log")
	}
	if err := verifyLogs(ctx, b, &r, fold, refs); err != nil {
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
	builds := map[string]string{} // build hash to the first record that names it
	for _, name := range names {
		r.Pointers++
		prev := ""
		for i, seq := range byName[name] {
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
	buildFile := map[string]string{} // file hex to the build that names it
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
	if err := verifyDescription(ctx, b, &r); err != nil {
		return r, err
	}
	d, err := readDescribed(ctx, b)
	if err != nil {
		return r, err
	}
	referencedBy := map[string]string{} // blob hex to what names it by a marked field
	fromLogs, err := d.logRefs(ctx, b)
	if err != nil {
		return r, err
	}
	for h, by := range fromLogs {
		if !named[h] {
			named[h] = true
			referencedBy[h] = by
		}
	}
	hexes := make([]string, 0, len(named))
	for h := range named {
		hexes = append(hexes, h)
	}
	sort.Strings(hexes)
	for i := 0; i < len(hexes); i++ {
		h := hexes[i]
		key := blobKey(h)
		data, meta, err := b.Get(ctx, key)
		if errors.Is(err, ErrNotFound) {
			if by, ok := buildFile[h]; ok {
				return r, broken(key, "build %s names it, and the blob is missing", by)
			}
			if by, ok := referencedBy[h]; ok {
				return r, broken(key, "%s names it in a content-hash field, and the blob is missing", by)
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
		for _, ref := range d.refs(env.Kind, env.SchemaVersion, data) {
			if !named[ref] {
				named[ref] = true
				referencedBy[ref] = key
				hexes = append(hexes, ref)
			}
		}
	}
	r.Referenced = len(referencedBy)
	if seqs := byName["manifest"]; len(seqs) > 0 {
		if err := verifyManifest(ctx, b, recordKey("manifest", seqs[len(seqs)-1]), named); err != nil {
			return r, err
		}
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

// manifestDocuments are the manifest's fields that name a document under
// a pointer. Folds that logs keep and the modules' hashes are not
// blobs, so verify does not look for them.
var manifestDocuments = []string{
	"feed_hash", "network_hash", "boundary_hash", "streets_hash", "walks_hash",
	"policy_hash", "build_hash",
}

func verifyManifest(ctx context.Context, b Backend, key string, named map[string]bool) error {
	raw, _, err := b.Get(ctx, key)
	if err != nil {
		return err
	}
	var e LogEntry
	if err := json.Unmarshal(raw, &e); err != nil {
		return broken(key, "the record is not JSON: %v", err)
	}
	h, _ := ParseHash(e.To)
	doc, _, err := b.Get(ctx, blobKey(h))
	if err != nil {
		return broken(key, "the head manifest does not read: %v", err)
	}
	var m map[string]any
	if err := json.Unmarshal(doc, &m); err != nil {
		return broken(blobKey(h), "the head manifest is not JSON: %v", err)
	}
	for _, field := range manifestDocuments {
		hash, _ := m[field].(string)
		if hash == "" {
			continue
		}
		x, err := ParseHash(hash)
		if err != nil {
			return broken(blobKey(h), "the head manifest's %s: %v", field, err)
		}
		if !named[x] {
			return broken(blobKey(h), "the head manifest's %s is %s, and no record names it", field, hash)
		}
	}
	return nil
}

type logHeader struct {
	Kind string `json:"kind"`
	Log  string `json:"log"`
	Seq  int64  `json:"seq"`
}

func verifyLogs(ctx context.Context, b Backend, r *VerifyReport, fold LogFolder, refs ReferenceChecker) error {
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
	heads, entries, seqs := map[string][]byte{}, map[string][]StoredEntry{}, map[string]int64{}
	read := func(key, name string, seq int64, kind string) error {
		raw, _, err := b.Get(ctx, key)
		if err != nil {
			return err
		}
		var h logHeader
		if err := json.Unmarshal(raw, &h); err != nil {
			return broken(key, "not JSON: %v", err)
		}
		if h.Kind != kind || h.Log != name || h.Seq != seq {
			return broken(key, "says kind %q, log %q, seq %d; its key says %s, %s, %d", h.Kind, h.Log, h.Seq, kind, name, seq)
		}
		return nil
	}
	for _, name := range names {
		l := logs[name]
		r.Logs++
		for i, seq := range l.entries {
			key := entryKey(name, seq)
			if seq != int64(i+1) {
				return broken(key, "a gap: log %s has no entry %d", name, i+1)
			}
			if err := read(key, name, seq, "log_entry"); err != nil {
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
			if err := read(key, name, seq, "log_checkpoint"); err != nil {
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
		if refs == nil || len(l.entries) == 0 {
			continue
		}
		all := make([]StoredEntry, 0, len(l.entries))
		for _, e := range l.entries {
			data, _, err := b.Get(ctx, entryKey(name, e))
			if err != nil {
				return err
			}
			all = append(all, StoredEntry{Seq: e, Text: string(data)})
		}
		head, err := fold(name, prev, all[prevSeq:])
		if err != nil {
			return broken(entryKey(name, int64(len(l.entries))), "the fold of entries %d..%d: %v", prevSeq+1, len(l.entries), err)
		}
		heads[name], entries[name], seqs[name] = head, all, int64(len(l.entries))
	}
	if refs == nil {
		return nil
	}
	found, err := refs(heads, entries)
	if err != nil {
		return fmt.Errorf("store verify: the existence check: %w", err)
	}
	found.Seqs = seqs
	r.References = found
	return nil
}

func verifyDescription(ctx context.Context, b Backend, r *VerifyReport) error {
	for _, prefix := range []string{"schemas/", "kinds/"} {
		objs, err := b.List(ctx, prefix, "")
		if err != nil {
			return err
		}
		for _, o := range objs {
			base := o.Key[strings.LastIndex(o.Key, "/")+1:]
			hex, ok := strings.CutSuffix(base, ".json")
			if _, err := ParseHash("sha256:" + hex); !ok || err != nil {
				return broken(o.Key, "not a description key (%s<…>/<sha256 hex>.json)", prefix)
			}
			data, _, err := b.Get(ctx, o.Key)
			if err != nil {
				return err
			}
			if got := hashOf(data); got != "sha256:"+hex {
				return broken(o.Key, "the bytes hash to %s", got)
			}
			if prefix == "schemas/" {
				r.Schemas++
			} else {
				r.Tables++
			}
		}
	}
	return nil
}
