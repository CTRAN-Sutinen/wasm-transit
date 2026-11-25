// Package store is the content-addressed document store the sync server
// keeps when run with -store (docs/architecture/storage.md, which holds
// the key layout; D-113).
//
// The pointer is its highest record: there is no pointer object. A
// commit creates the blob, then creates record N+1, which succeeds only
// when no record N+1 exists. That conditional create is the CAS, so any
// number of processes may share one bucket. A crash between the creates
// orphans the blob, and the offline collector (gc.go) deletes it later.
//
// The invariants are model-checked for processes that share a bucket
// with the offline GC (store.tla, `just spec`): every pointer, current
// or historical, names a durable blob. Each record's `from` is the
// prior `to`. GC never deletes a referenced blob. A failed CAS changes
// nothing.
package store

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"strconv"
	"strings"
	"sync"
	"time"
)

// OrphanGrace is how long an unreferenced blob survives before GC
// collects it. It must outlast a crash in the middle of a commit.
const OrphanGrace = 7 * 24 * time.Hour

const OrphanGraceMs = int64(OrphanGrace / time.Millisecond)

// FreshnessWindow bounds how far a read may trail another process's
// commit. A read probes for the next record at most once per window per
// pointer, and only when a read asks.
const FreshnessWindow = 2 * time.Second

const envelopeMeta = "envelope"

const fetchWorkers = 16

type Envelope struct {
	Kind          string `json:"kind"` // network_document, edit_records, diversion_draft, manifest, plans, policy
	SchemaVersion uint32 `json:"schema_version"`
	ProducedBy    string `json:"produced_by"` // "cap-sync@sha256:<hex>", or "server" for host-owned documents
}

type LogEntry struct {
	Seq           int64   `json:"seq"`
	From          string  `json:"from"` // the prior `to`; "" on create
	To            string  `json:"to"`
	Kind          string  `json:"kind"`
	SchemaVersion uint32  `json:"schema_version"`
	ProducedBy    string  `json:"produced_by"`
	Writer        string  `json:"writer"` // reserved for cap-authz
	Policy        *string `json:"policy"` // reserved for cap-authz
	AtMs          int64   `json:"at_ms"`  // host clock, Unix epoch ms
}

// While the bucket does not answer, a Head is the cached one, marked
// Stale, as of the last time the bucket confirmed it (D-119, D-224).
type Head struct {
	Name     string   `json:"name"`
	Hash     string   `json:"hash"` // "" when the pointer does not exist
	Seq      int64    `json:"seq"`
	Envelope Envelope `json:"envelope"`
	AtMs     int64    `json:"at_ms"`           // the head record's commit time
	AsOfMs   int64    `json:"as_of_ms"`        // when the bucket last confirmed it is the head
	Stale    bool     `json:"stale,omitempty"` // the bucket did not answer this read
}

type ConflictError struct {
	Pointer string
	Current string
	Seq     int64
}

func (e *ConflictError) Error() string {
	return fmt.Sprintf("store: pointer %q moved (current %s, seq %d)", e.Pointer, e.Current, e.Seq)
}

var ErrNotFound = errors.New("store: not found")

// pointer is this process's view of one pointer. Records are immutable,
// so a cached record is final.
type pointer struct {
	mu     sync.Mutex // serializes this process's refreshes and commits
	loaded bool
	seq    int64
	to     string
	env    Envelope
	atMs   int64
	recs   map[int64]LogEntry
	probed time.Time
	// fromCache means the head came from the disk cache. The next read
	// that the bucket answers lists the pointer again from the start.
	fromCache bool
}

type Store struct {
	b       Backend
	serving bool
	cache   *blobCache
	disk    *diskCache // nil: no disk cache (KeepCache)
	mu      sync.Mutex
	ptrs    map[string]*pointer
	logs    map[string]*logState // the append-only logs (log.go)
	now     func() time.Time
	// afterList runs between a GC pass's listing and its deletes, so a
	// test can land a commit there.
	afterList func()
}

func Open(url string) (*Store, error) {
	b, err := OpenBackend(context.Background(), url)
	if err != nil {
		return nil, err
	}
	return New(b), nil
}

func OpenServing(url string) (*Store, error) {
	b, err := OpenBackend(context.Background(), url)
	if err != nil {
		return nil, err
	}
	return NewServing(b), nil
}

func New(b Backend) *Store {
	return &Store{b: guarded{b}, cache: newBlobCache(BlobCacheBytes), ptrs: map[string]*pointer{}, now: time.Now}
}

// NewServing refuses every delete and GC, whatever the credentials
// allow. A compromised server then cannot erase history (D-117).
func NewServing(b Backend) *Store {
	s := New(servingBackend{b})
	s.serving = true
	return s
}

var ErrServingNeverDeletes = errors.New("store: a serving process never deletes (D-117; run `store gc`)")

type servingBackend struct{ Backend }

func (servingBackend) Delete(context.Context, string) error { return ErrServingNeverDeletes }

func (s *Store) Close() error { return nil }

// ValidPointerName accepts one key segment that cannot reach another
// pointer's records or hide as a dotfile.
func ValidPointerName(name string) bool {
	if name == "" || len(name) > 128 {
		return false
	}
	for i, c := range name {
		alnum := c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9'
		if i == 0 && !alnum {
			return false
		}
		if !alnum && c != '.' && c != '_' && c != '-' {
			return false
		}
	}
	return true
}

func ParseHash(hash string) (string, error) {
	if !strings.HasPrefix(hash, "sha256:") {
		return "", fmt.Errorf("not a content hash: %q", hash)
	}
	hexPart := strings.TrimPrefix(hash, "sha256:")
	if len(hexPart) != 64 {
		return "", fmt.Errorf("not a sha256 hex: %q", hash)
	}
	if _, err := hex.DecodeString(hexPart); err != nil {
		return "", fmt.Errorf("not a sha256 hex: %q", hash)
	}
	return hexPart, nil
}

func HashOf(data []byte) string { return hashOf(data) }

func hashOf(data []byte) string {
	sum := sha256.Sum256(data)
	return "sha256:" + hex.EncodeToString(sum[:])
}

func blobKey(h string) string         { return "blobs/" + h }
func recordPrefix(name string) string { return "pointers/" + name + "/" }
func recordKey(name string, seq int64) string {
	return fmt.Sprintf("%s%020d.json", recordPrefix(name), seq)
}

func recordSeq(key string) (name string, seq int64, ok bool) {
	rest, found := strings.CutPrefix(key, "pointers/")
	if !found {
		return "", 0, false
	}
	name, file, found := strings.Cut(rest, "/")
	digits, isJSON := strings.CutSuffix(file, ".json")
	if !found || !isJSON || len(digits) != 20 || !ValidPointerName(name) {
		return "", 0, false
	}
	seq, err := strconv.ParseInt(digits, 10, 64)
	if err != nil || seq < 1 {
		return "", 0, false
	}
	return name, seq, true
}

// PutBlob stores data under its content hash. wantHash, when non-empty,
// is the hash that the module computed. The store checks it against
// sha256(data) and never computes a bus document's identity itself.
func (s *Store) PutBlob(data []byte, env Envelope, wantHash string) (string, error) {
	return s.putBlob(data, env, wantHash)
}

func (s *Store) putBlob(data []byte, env Envelope, wantHash string) (string, error) {
	hash := hashOf(data)
	if wantHash != "" && wantHash != hash {
		return "", fmt.Errorf("hash mismatch: module says %s, bytes are %s", wantHash, hash)
	}
	h, _ := ParseHash(hash)
	ctx := context.Background()
	if _, err := s.b.Stat(ctx, blobKey(h)); err == nil {
		s.keepBlob(hash, data, env)
		return hash, nil // idempotent, without re-sending the bytes
	} else if !errors.Is(err, ErrNotFound) {
		return "", err
	}
	meta, err := json.Marshal(env)
	if err != nil {
		return "", err
	}
	err = s.b.Create(ctx, blobKey(h), data, map[string]string{envelopeMeta: string(meta)})
	if err != nil && !errors.Is(err, ErrExists) {
		return "", err
	}
	s.keepBlob(hash, data, env)
	return hash, nil
}

func (s *Store) keepBlob(hash string, data []byte, env Envelope) {
	s.cache.put(hash, data, env)
	s.disk.putBlob(hash, data, env)
}

func (s *Store) GetBlob(hash string) ([]byte, Envelope, error) {
	h, err := ParseHash(hash)
	if err != nil {
		return nil, Envelope{}, err
	}
	if c, ok := s.cache.get(hash); ok {
		s.disk.touch(hash)
		return c.data, c.env, nil // immutable: a cached blob is always correct
	}
	data, meta, err := s.b.Get(context.Background(), blobKey(h))
	if errors.Is(err, ErrUnavailable) {
		if data, env, ok := s.disk.getBlob(hash); ok {
			s.cache.put(hash, data, env)
			return data, env, nil
		}
	}
	if err != nil {
		return nil, Envelope{}, err
	}
	var env Envelope
	if err := json.Unmarshal([]byte(meta[envelopeMeta]), &env); err != nil {
		return nil, Envelope{}, fmt.Errorf("blob %s envelope: %w", hash, err)
	}
	s.keepBlob(hash, data, env)
	return data, env, nil
}

func (s *Store) pointer(name string) *pointer {
	s.mu.Lock()
	defer s.mu.Unlock()
	p, ok := s.ptrs[name]
	if !ok {
		p = &pointer{recs: map[int64]LogEntry{}}
		s.ptrs[name] = p
	}
	return p
}

// Caller holds p.mu.
func (s *Store) getRecord(p *pointer, name string, seq int64) (LogEntry, error) {
	if e, ok := p.recs[seq]; ok {
		return e, nil
	}
	raw, _, err := s.b.Get(context.Background(), recordKey(name, seq))
	if err != nil {
		return LogEntry{}, fmt.Errorf("pointer %s record %d: %w", name, seq, err)
	}
	var e LogEntry
	if err := json.Unmarshal(raw, &e); err != nil {
		return LogEntry{}, fmt.Errorf("pointer %s record %d: %w", name, seq, err)
	}
	if e.Seq != seq {
		return LogEntry{}, fmt.Errorf("pointer %s record %d says seq %d", name, seq, e.Seq)
	}
	p.recs[seq] = e
	return e, nil
}

// Caller holds p.mu.
func (s *Store) loadRange(p *pointer, name string, lo, hi int64) error {
	var missing []int64
	for seq := lo; seq <= hi; seq++ {
		if _, ok := p.recs[seq]; !ok {
			missing = append(missing, seq)
		}
	}
	if len(missing) == 0 {
		return nil
	}
	type got struct {
		e   LogEntry
		err error
	}
	results := make([]got, len(missing))
	var wg sync.WaitGroup
	next := make(chan int)
	for w := 0; w < fetchWorkers && w < len(missing); w++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for i := range next {
				raw, _, err := s.b.Get(context.Background(), recordKey(name, missing[i]))
				if err == nil {
					err = json.Unmarshal(raw, &results[i].e)
				}
				results[i].err = err
			}
		}()
	}
	for i := range missing {
		next <- i
	}
	close(next)
	wg.Wait()
	for i, r := range results {
		if r.err != nil {
			return fmt.Errorf("pointer %s record %d: %w", name, missing[i], r.err)
		}
		if r.e.Seq != missing[i] {
			return fmt.Errorf("pointer %s record %d says seq %d", name, missing[i], r.e.Seq)
		}
		p.recs[missing[i]] = r.e
	}
	return nil
}

// Caller holds p.mu.
func (s *Store) advance(p *pointer, name string) error {
	after := ""
	if p.seq > 0 {
		after = recordKey(name, p.seq)
	}
	objs, err := s.b.List(context.Background(), recordPrefix(name), after)
	if err != nil {
		return err
	}
	top := p.seq
	for _, o := range objs {
		if n, seq, ok := recordSeq(o.Key); ok && n == name && seq > top {
			top = seq
		}
	}
	if top > p.seq {
		// A first load reads only the head; a refresh reads what it missed
		// (and checks the chain across it).
		lo := p.seq + 1
		if p.seq == 0 {
			lo = top
		}
		if err := s.loadRange(p, name, lo, top); err != nil {
			return err
		}
		// A first load past seq 1 has no prior record in hand to chain to.
		prev, unchained := p.to, p.seq == 0 && lo > 1
		for seq := lo; seq <= top; seq++ {
			e := p.recs[seq]
			if !(unchained && seq == lo) && e.From != prev {
				return fmt.Errorf("pointer %s log broken at seq %d", name, seq)
			}
			prev = e.To
		}
		head := p.recs[top]
		p.seq, p.to, p.atMs = top, head.To, head.AtMs
		p.env = Envelope{Kind: head.Kind, SchemaVersion: head.SchemaVersion, ProducedBy: head.ProducedBy}
	}
	p.loaded = true
	p.probed = s.now()
	s.keepHead(p, name)
	return nil
}

// Caller holds p.mu.
func (s *Store) keepHead(p *pointer, name string) {
	s.disk.putHead(name, cachedHead{Seq: p.seq, To: p.to, Envelope: p.env, AtMs: p.atMs, AsOfMs: p.probed.UnixMilli()})
}

// Caller holds p.mu.
func (s *Store) fromDisk(p *pointer, name string) bool {
	h, ok := s.disk.head(name)
	if !ok {
		return false
	}
	p.seq, p.to, p.env, p.atMs = h.Seq, h.To, h.Envelope, h.AtMs
	p.probed = time.UnixMilli(h.AsOfMs)
	p.loaded, p.fromCache = true, true
	return true
}

// relist keeps the cache's head when the listing fails. It requires
// p.mu.
func (s *Store) relist(p *pointer, name string) error {
	seq, to, env, atMs, probed := p.seq, p.to, p.env, p.atMs, p.probed
	p.seq, p.to, p.env, p.atMs, p.loaded = 0, "", Envelope{}, 0, false
	if err := s.advance(p, name); err != nil {
		p.seq, p.to, p.env, p.atMs, p.probed, p.loaded = seq, to, env, atMs, probed, true
		return err
	}
	p.fromCache = false
	return nil
}

// fresh requires p.mu. After the first listing, it makes at most one
// Stat of record seq+1 per window. An absent record means the cache is
// current.
func (s *Store) fresh(p *pointer, name string) error {
	if !p.loaded {
		return s.advance(p, name)
	}
	if p.fromCache {
		return s.relist(p, name)
	}
	if s.now().Sub(p.probed) < FreshnessWindow {
		return nil
	}
	if _, err := s.b.Stat(context.Background(), recordKey(name, p.seq+1)); errors.Is(err, ErrNotFound) {
		p.probed = s.now()
		s.keepHead(p, name)
		return nil
	} else if err != nil {
		return err
	}
	return s.advance(p, name)
}

func (p *pointer) head(name string) Head {
	return Head{Name: name, Hash: p.to, Seq: p.seq, Envelope: p.env, AtMs: p.atMs, AsOfMs: p.probed.UnixMilli()}
}

func (s *Store) Head(name string) (Head, bool, error) {
	if !ValidPointerName(name) {
		return Head{Name: name}, false, nil
	}
	p := s.pointer(name)
	p.mu.Lock()
	defer p.mu.Unlock()
	if err := s.fresh(p, name); err != nil {
		if errors.Is(err, ErrUnavailable) && !p.loaded {
			s.fromDisk(p, name) // a copy booted while the bucket does not answer
		}
		if !errors.Is(err, ErrUnavailable) || !p.loaded {
			return Head{Name: name}, false, err
		}
		// The bucket did not answer, so serve the cached head, marked Stale (D-119).
		if p.seq == 0 {
			return Head{Name: name, Stale: true, AsOfMs: p.probed.UnixMilli()}, false, nil
		}
		h := p.head(name)
		h.Stale = true
		return h, true, nil
	}
	if p.seq == 0 {
		return Head{Name: name}, false, nil
	}
	return p.head(name), true, nil
}

// CAS commits data as the pointer's next value. from is the hash that
// the writer last saw, and "" means the pointer must not exist. After a
// commit, CAS reads the blob again and re-creates it if a GC pass
// removed it in between (store.tla).
func (s *Store) CAS(name, from string, data []byte, env Envelope, wantHash, writer string, atMs int64) (string, int64, error) {
	if !ValidPointerName(name) {
		return "", 0, fmt.Errorf("store: %q is not a pointer name", name)
	}
	hash := hashOf(data)
	if wantHash != "" && wantHash != hash {
		return "", 0, fmt.Errorf("hash mismatch: module says %s, bytes are %s", wantHash, hash)
	}
	p := s.pointer(name)
	p.mu.Lock()
	defer p.mu.Unlock()
	if err := s.fresh(p, name); err != nil {
		return "", 0, err
	}
	moved := func() bool {
		if from == "" {
			return p.seq > 0
		}
		return p.to != from
	}
	if moved() {
		// The writer may have read a newer head through another process.
		if err := s.advance(p, name); err != nil {
			return "", 0, err
		}
		if moved() {
			return "", 0, &ConflictError{Pointer: name, Current: p.to, Seq: p.seq}
		}
	}
	if p.seq > 0 && p.to == hash {
		// Redelivery of the head adds no record, so at-least-once
		// delivery converges to the same hash and the same seq.
		return hash, p.seq, nil
	}
	if _, err := s.putBlob(data, env, ""); err != nil {
		return "", 0, err
	}
	entry := LogEntry{
		Seq: p.seq + 1, From: p.to, To: hash,
		Kind: env.Kind, SchemaVersion: env.SchemaVersion, ProducedBy: env.ProducedBy,
		Writer: writer, AtMs: atMs,
	}
	body, err := json.Marshal(entry)
	if err != nil {
		return "", 0, err
	}
	if err := s.b.Create(context.Background(), recordKey(name, entry.Seq), body, nil); errors.Is(err, ErrExists) {
		// Another process took seq N+1 first: its commit stands.
		if err := s.advance(p, name); err != nil {
			return "", 0, err
		}
		if p.to == hash {
			return hash, p.seq, nil // it committed these very bytes
		}
		return "", 0, &ConflictError{Pointer: name, Current: p.to, Seq: p.seq}
	} else if err != nil {
		return "", 0, err
	}
	p.recs[entry.Seq] = entry
	p.seq, p.to, p.env, p.atMs = entry.Seq, hash, env, atMs
	p.probed = s.now()
	s.keepHead(p, name)
	// A commit that reused an aged orphan may have had it deleted under
	// it. The re-create is the same object. A crash before this line is
	// for `store verify` to find.
	if _, err := s.b.Stat(context.Background(), blobKey(strings.TrimPrefix(hash, "sha256:"))); errors.Is(err, ErrNotFound) {
		if _, err := s.putBlob(data, env, ""); err != nil {
			return "", 0, fmt.Errorf("commit %s seq %d: re-create its blob: %w", name, entry.Seq, err)
		}
	} else if err != nil {
		return "", 0, fmt.Errorf("commit %s seq %d: re-read its blob: %w", name, entry.Seq, err)
	}
	return hash, entry.Seq, nil
}

// Log returns the records after the latest record whose To is since,
// or every record when since is "". found is false when since is not in
// the log. The caller then falls back to the whole current document.
func (s *Store) Log(name, since string) (entries []LogEntry, found bool, err error) {
	if !ValidPointerName(name) {
		return nil, false, nil
	}
	p := s.pointer(name)
	p.mu.Lock()
	defer p.mu.Unlock()
	if err := s.fresh(p, name); err != nil {
		return nil, false, err
	}
	if p.seq == 0 {
		return nil, false, nil
	}
	collect := func(lo int64) []LogEntry {
		out := make([]LogEntry, 0, p.seq-lo+1)
		for seq := lo; seq <= p.seq; seq++ {
			out = append(out, p.recs[seq])
		}
		return out
	}
	if since == "" {
		if err := s.loadRange(p, name, 1, p.seq); err != nil {
			return nil, false, err
		}
		return collect(1), true, nil
	}
	// Walk back from the head a chunk at a time: a client is usually a
	// few records behind.
	const chunk = 64
	for hi := p.seq; hi >= 1; hi -= chunk {
		lo := max(hi-chunk+1, 1)
		if err := s.loadRange(p, name, lo, hi); err != nil {
			return nil, false, err
		}
		for seq := hi; seq >= lo; seq-- {
			if p.recs[seq].To == since {
				return collect(seq + 1), true, nil
			}
		}
	}
	return nil, false, nil
}

// Input reads inputs/<key>, which an operator places in the bucket.
// The store never writes there (D-118).
func (s *Store) Input(key string) ([]byte, ObjectInfo, error) {
	if key == "" || strings.HasPrefix(key, "/") || strings.Contains(key, "..") {
		return nil, ObjectInfo{}, fmt.Errorf("store: %q is not an input key", key)
	}
	ctx := context.Background()
	info, err := s.b.Stat(ctx, "inputs/"+key)
	if err != nil {
		return nil, ObjectInfo{}, err
	}
	data, _, err := s.b.Get(ctx, "inputs/"+key)
	return data, info, err
}

func (s *Store) InputModified(key string) (ObjectInfo, error) {
	return s.b.Stat(context.Background(), "inputs/"+key)
}
