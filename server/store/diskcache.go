package store

// The disk cache (-store-cache): a server copy's local working copy of
// what it read from the bucket and confirmed. A copy that starts while
// the bucket does not answer boots on it read-only (D-119): it serves
// the heads it last confirmed, labelled stale, and the blobs, log
// entries, and checkpoints it holds, and it refuses every write. The
// cache is never the record. Nothing in it is written back to the
// bucket, and losing it loses nothing.
//
// Layout (under <dir>/<scope>, one scope for each store URL):
//
//	store-url                           the store URL this cache is of
//	blobs/<hex>, blobs/<hex>.envelope   a blob and its envelope
//	heads/<pointer>.json                a pointer's last confirmed head
//	logs/<log>/head.json                a log's last confirmed head
//	logs/<log>/entries/<seq>.json       an entry, as the bucket holds it
//	logs/<log>/checkpoints/<seq>.json   a checkpoint, as the bucket holds it
//
// Every object the cache holds is immutable in the bucket, except a
// head. A head read from the cache is checked again from the start when
// the bucket answers, so a bucket restored to an older state is never
// served past the cache's head. Each write is a whole file, renamed into
// place. A failed cache write is ignored: the bucket stays the record.

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"
)

// DiskCacheBytes bounds the blobs the disk cache keeps. The least
// recently used blob goes first past the bound. A C-TRAN pick (network
// 9.6 MB, streets 22 MB) and every build fit many times over.
const DiskCacheBytes = 4 << 30

// cacheAsOfEvery bounds how often a confirmed head that did not move is
// written again. The cache's as-of then trails the truth by at most
// this, so a stale label never says newer than it is.
const cacheAsOfEvery = 30 * time.Second

// diskCache is the files under one scope.
type diskCache struct {
	dir string
	max int64

	mu    sync.Mutex
	total int64
	sizes map[string]int64     // blob hex → bytes
	used  map[string]time.Time // blob hex → last use
	wrote map[string]wroteHead // a pointer's or log's last head write
}

type wroteHead struct {
	state string
	at    time.Time
}

// cachedHead is a pointer's head as the cache holds it. Seq 0 is a
// pointer the bucket confirmed has no commits.
type cachedHead struct {
	Seq      int64    `json:"seq"`
	To       string   `json:"to"`
	Envelope Envelope `json:"envelope"`
	AtMs     int64    `json:"at_ms"`
	AsOfMs   int64    `json:"as_of_ms"`
}

// CacheScope names the folder under a -store-cache directory that
// holds one store URL's cache: two stores never share one.
func CacheScope(storeURL string) string {
	sum := sha256.Sum256([]byte(storeURL))
	return hex.EncodeToString(sum[:8])
}

// KeepCache makes the store keep a disk cache under dir, scoped to
// storeURL (CacheScope). It fails when dir cannot be written, so an
// operator learns of a bad setting at boot, not in an outage.
func (s *Store) KeepCache(dir, storeURL string) error {
	root := filepath.Join(dir, CacheScope(storeURL))
	for _, sub := range []string{"blobs", "heads", "logs"} {
		if err := os.MkdirAll(filepath.Join(root, sub), 0o755); err != nil {
			return fmt.Errorf("store cache: %w", err)
		}
	}
	if err := writeFileAtomic(filepath.Join(root, "store-url"), []byte(storeURL+"\n")); err != nil {
		return fmt.Errorf("store cache: %w", err)
	}
	c := &diskCache{dir: root, max: DiskCacheBytes, sizes: map[string]int64{}, used: map[string]time.Time{}, wrote: map[string]wroteHead{}}
	files, err := os.ReadDir(filepath.Join(root, "blobs"))
	if err != nil {
		return fmt.Errorf("store cache: %w", err)
	}
	for _, f := range files {
		name := f.Name()
		if strings.Contains(name, ".") {
			continue
		}
		info, err := f.Info()
		if err != nil {
			continue
		}
		c.sizes[name] = info.Size()
		c.used[name] = info.ModTime()
		c.total += info.Size()
	}
	s.disk = c
	return nil
}

// writeFileAtomic writes a whole file: a temporary file, then a rename.
func writeFileAtomic(path string, data []byte) error {
	tmp, err := os.CreateTemp(filepath.Dir(path), ".tmp-*")
	if err != nil {
		return err
	}
	_, err = tmp.Write(data)
	if cerr := tmp.Close(); err == nil {
		err = cerr
	}
	if err == nil {
		err = os.Rename(tmp.Name(), path)
	}
	if err != nil {
		os.Remove(tmp.Name())
	}
	return err
}

func (c *diskCache) blobPath(h string) string { return filepath.Join(c.dir, "blobs", h) }

// putBlob keeps a blob, once. Then it evicts the least recently used
// blobs past the bound.
func (c *diskCache) putBlob(hash string, data []byte, env Envelope) {
	if c == nil || int64(len(data)) > c.max {
		return
	}
	h := strings.TrimPrefix(hash, "sha256:")
	c.mu.Lock()
	_, have := c.sizes[h]
	c.used[h] = time.Now()
	c.mu.Unlock()
	if have {
		return
	}
	meta, err := json.Marshal(env)
	if err != nil {
		return
	}
	// The envelope first: a blob file is only read with its envelope.
	if writeFileAtomic(c.blobPath(h)+".envelope", meta) != nil || writeFileAtomic(c.blobPath(h), data) != nil {
		return
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if _, have := c.sizes[h]; !have {
		c.sizes[h] = int64(len(data))
		c.total += int64(len(data))
	}
	if c.total <= c.max {
		return
	}
	order := make([]string, 0, len(c.sizes))
	for k := range c.sizes {
		if k != h {
			order = append(order, k)
		}
	}
	sort.Slice(order, func(i, j int) bool { return c.used[order[i]].Before(c.used[order[j]]) })
	for _, k := range order {
		if c.total <= c.max {
			break
		}
		os.Remove(c.blobPath(k))
		os.Remove(c.blobPath(k) + ".envelope")
		c.total -= c.sizes[k]
		delete(c.sizes, k)
		delete(c.used, k)
	}
}

// touch marks a blob used (a read served from memory).
func (c *diskCache) touch(hash string) {
	if c == nil {
		return
	}
	h := strings.TrimPrefix(hash, "sha256:")
	c.mu.Lock()
	if _, have := c.sizes[h]; have {
		c.used[h] = time.Now()
	}
	c.mu.Unlock()
}

// getBlob reads a kept blob and its envelope.
func (c *diskCache) getBlob(hash string) ([]byte, Envelope, bool) {
	if c == nil {
		return nil, Envelope{}, false
	}
	h := strings.TrimPrefix(hash, "sha256:")
	meta, err := os.ReadFile(c.blobPath(h) + ".envelope")
	if err != nil {
		return nil, Envelope{}, false
	}
	data, err := os.ReadFile(c.blobPath(h))
	if err != nil || hashOf(data) != "sha256:"+h {
		return nil, Envelope{}, false
	}
	var env Envelope
	if json.Unmarshal(meta, &env) != nil {
		return nil, Envelope{}, false
	}
	c.touch(hash)
	return data, env, true
}

// due reports whether a head in state is to be written: it changed, or
// the last write is older than cacheAsOfEvery.
func (c *diskCache) due(key, state string) bool {
	c.mu.Lock()
	defer c.mu.Unlock()
	w, ok := c.wrote[key]
	if ok && w.state == state && time.Since(w.at) < cacheAsOfEvery {
		return false
	}
	c.wrote[key] = wroteHead{state: state, at: time.Now()}
	return true
}

// putHead keeps a pointer's confirmed head.
func (c *diskCache) putHead(name string, h cachedHead) {
	if c == nil || !c.due("pointer/"+name, fmt.Sprint(h.Seq)) {
		return
	}
	data, _ := json.Marshal(h)
	writeFileAtomic(filepath.Join(c.dir, "heads", name+".json"), data)
}

// head reads a pointer's kept head.
func (c *diskCache) head(name string) (cachedHead, bool) {
	if c == nil {
		return cachedHead{}, false
	}
	data, err := os.ReadFile(filepath.Join(c.dir, "heads", name+".json"))
	var h cachedHead
	if err != nil || json.Unmarshal(data, &h) != nil {
		return cachedHead{}, false
	}
	return h, true
}

func (c *diskCache) logDir(name string) string { return filepath.Join(c.dir, "logs", name) }
func seqFile(seq int64) string                 { return fmt.Sprintf("%020d.json", seq) }

// putLogHead keeps a log's confirmed head.
func (c *diskCache) putLogHead(name string, h LogHead) {
	if c == nil || !c.due("log/"+name, fmt.Sprint(h.Seq, h.Gaps, h.Checkpoint)) {
		return
	}
	if os.MkdirAll(c.logDir(name), 0o755) != nil {
		return
	}
	data, _ := json.Marshal(h)
	writeFileAtomic(filepath.Join(c.logDir(name), "head.json"), data)
}

// logHead reads a log's kept head.
func (c *diskCache) logHead(name string) (LogHead, bool) {
	if c == nil {
		return LogHead{}, false
	}
	data, err := os.ReadFile(filepath.Join(c.logDir(name), "head.json"))
	var h LogHead
	if err != nil || json.Unmarshal(data, &h) != nil {
		return LogHead{}, false
	}
	return h, true
}

// putLogObject keeps an entry or a checkpoint (what is "entries" or
// "checkpoints"), once.
func (c *diskCache) putLogObject(name, what string, seq int64, data []byte) {
	if c == nil {
		return
	}
	dir := filepath.Join(c.logDir(name), what)
	path := filepath.Join(dir, seqFile(seq))
	if _, err := os.Stat(path); err == nil {
		return
	}
	if os.MkdirAll(dir, 0o755) != nil || writeFileAtomic(path, data) != nil {
		return
	}
	if what == "checkpoints" {
		c.prune(name)
	}
}

// logObject reads a kept entry or checkpoint.
func (c *diskCache) logObject(name, what string, seq int64) ([]byte, bool) {
	if c == nil {
		return nil, false
	}
	data, err := os.ReadFile(filepath.Join(c.logDir(name), what, seqFile(seq)))
	return data, err == nil
}

// logSeqs lists the seqs of the kept entries or checkpoints, ascending.
func (c *diskCache) logSeqs(name, what string) []int64 {
	if c == nil {
		return nil
	}
	files, _ := os.ReadDir(filepath.Join(c.logDir(name), what))
	var out []int64
	for _, f := range files {
		var seq int64
		if n, _ := fmt.Sscanf(f.Name(), "%020d.json", &seq); n == 1 && seqFile(seq) == f.Name() {
			out = append(out, seq)
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i] < out[j] })
	return out
}

// prune keeps what a boot reads: the newest two checkpoints, and the
// entries after the older of them. A boot checks the newest checkpoint
// against the fold from the one before it.
func (c *diskCache) prune(name string) {
	cps := c.logSeqs(name, "checkpoints")
	if len(cps) < 2 {
		return
	}
	keepFrom := cps[len(cps)-2]
	for _, seq := range cps[:len(cps)-2] {
		os.Remove(filepath.Join(c.logDir(name), "checkpoints", seqFile(seq)))
	}
	for _, seq := range c.logSeqs(name, "entries") {
		if seq > keepFrom {
			break
		}
		os.Remove(filepath.Join(c.logDir(name), "entries", seqFile(seq)))
	}
}
