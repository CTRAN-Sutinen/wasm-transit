package store

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// CopyReport is what a directory copy carried.
type CopyReport struct {
	Records  int          `json:"records"`
	Blobs    int          `json:"blobs"`   // named blobs carried
	Dropped  int          `json:"dropped"` // orphan blobs no record names (not carried)
	Verified VerifyReport `json:"verified"`
}

// CopyDir carries a store an older build kept in a directory (`store
// copy <dir> <url>`) into a bucket, then verifies the bucket. The old
// layout: blobs/<hex> with its envelope in blobs/<hex>.meta.json, and
// pointers/<name>.log, one JSON record per line. Each line becomes its
// record object, byte for byte; each blob a record names becomes its
// blob object with the sidecar as metadata. Orphans, .tmp leftovers and
// the lock file stay behind. A torn line is refused, never guessed at;
// a re-run is a no-op; a bucket holding another history is refused.
func CopyDir(ctx context.Context, dir string, dst Backend) (CopyReport, error) {
	var r CopyReport
	logs, err := filepath.Glob(filepath.Join(dir, "pointers", "*.log"))
	if err != nil {
		return r, err
	}
	sort.Strings(logs)
	named := map[string]bool{}
	for _, path := range logs {
		name := strings.TrimSuffix(filepath.Base(path), ".log")
		if !ValidPointerName(name) {
			return r, fmt.Errorf("%s: %q is not a pointer name", path, name)
		}
		f, err := os.Open(path)
		if err != nil {
			return r, err
		}
		sc := bufio.NewScanner(f)
		sc.Buffer(make([]byte, 1<<20), 1<<20)
		prev, line := "", 0
		for sc.Scan() {
			line++
			raw := bytes.TrimSpace(sc.Bytes())
			var e LogEntry
			if err := json.Unmarshal(raw, &e); err != nil {
				f.Close()
				return r, fmt.Errorf("pointers/%s.log line %d is torn or not a record (%v): repair the log by hand before copying", name, line, err)
			}
			if e.Seq != int64(line) || e.From != prev {
				f.Close()
				return r, fmt.Errorf("pointers/%s.log line %d: seq %d from %q does not chain", name, line, e.Seq, e.From)
			}
			if err := createSame(ctx, dst, recordKey(name, e.Seq), raw, nil); err != nil {
				f.Close()
				return r, err
			}
			h, err := ParseHash(e.To)
			if err != nil {
				f.Close()
				return r, fmt.Errorf("pointers/%s.log line %d: %w", name, line, err)
			}
			named[h] = true
			prev = e.To
			r.Records++
		}
		f.Close()
		if err := sc.Err(); err != nil {
			return r, fmt.Errorf("pointers/%s.log: %w", name, err)
		}
	}
	hexes := make([]string, 0, len(named))
	for h := range named {
		hexes = append(hexes, h)
	}
	sort.Strings(hexes)
	for _, h := range hexes {
		data, err := os.ReadFile(filepath.Join(dir, "blobs", h))
		if err != nil {
			return r, fmt.Errorf("blob %s, which a record names: %w", h, err)
		}
		meta, err := os.ReadFile(filepath.Join(dir, "blobs", h+".meta.json"))
		if err != nil {
			return r, fmt.Errorf("blob %s's envelope: %w", h, err)
		}
		var env Envelope
		if err := json.Unmarshal(meta, &env); err != nil {
			return r, fmt.Errorf("blob %s's envelope: %w", h, err)
		}
		canon, _ := json.Marshal(env)
		if err := dst.Create(ctx, blobKey(h), data, map[string]string{envelopeMeta: string(canon)}); err != nil && !errors.Is(err, ErrExists) {
			return r, err
		}
		r.Blobs++
	}
	entries, err := os.ReadDir(filepath.Join(dir, "blobs"))
	if err != nil {
		return r, err
	}
	for _, e := range entries {
		n := e.Name()
		if !strings.Contains(n, ".") && !named[n] {
			r.Dropped++
		}
	}
	r.Verified, err = Verify(ctx, dst)
	return r, err
}

// createSame creates key, or accepts it already holding these bytes (a
// re-run); different bytes are another history, never overwritten.
func createSame(ctx context.Context, b Backend, key string, data []byte, meta map[string]string) error {
	err := b.Create(ctx, key, data, meta)
	if !errors.Is(err, ErrExists) {
		return err
	}
	have, _, err := b.Get(ctx, key)
	if err != nil {
		return err
	}
	if !bytes.Equal(have, data) {
		return fmt.Errorf("%s: the bucket holds another history there; copy into an empty prefix", key)
	}
	return nil
}
