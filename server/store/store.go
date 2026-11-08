// Package store is the content-addressed document store the sync
// server keeps when run with -store (docs/ARCHITECTURE.md, server
// section; docs/FORMATS.md). Its home is an object store (D-113): the
// engine runs over a Backend (backend.go) and assumes only its
// contract — a whole-object create of an absent key, a consistent read
// and list after write, metadata, delete, a modified time.
//
// Layout (keys under the store URL's prefix):
//
//	blobs/<hex>                            one immutable blob — the document's
//	                                       exact bytes; its envelope is the
//	                                       object's metadata
//	pointers/<name>/<seq, 20 digits>.json  one commit record (LogEntry)
//
// The pointer is its highest record — there is no pointer object. A
// commit is two creates: the blob, then record N+1, which lands only
// when no record N+1 exists — the conditional create is the CAS, so any
// number of processes may share one bucket — and then a re-read of its
// blob, re-created if missing. A crash between the creates orphans the
// blob; the offline collector (gc.go, `store gc`) deletes blobs no
// record names, past the grace period. Serving processes never delete
// (D-117). Nothing is ever overwritten, appended, renamed, or locked.
// The store never mints document identity for bus documents: callers
// pass the module-produced hash and the store asserts sha256(bytes) ==
// hash, failing loudly on mismatch.
//
// The invariants: every pointer, current or historical, names a
// durable blob; the log is append-only and each record's `from` is the
// prior `to`; GC never deletes a referenced blob; a failed CAS leaves
// authority untouched — model-checked for processes sharing a bucket
// with the offline GC beside them (store.tla, `just spec`).
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
// collects it — long enough that a crash mid-commit is never data
// loss, short enough that orphans do not pile up.
const OrphanGrace = 7 * 24 * time.Hour

// OrphanGraceMs is OrphanGrace in milliseconds.
const OrphanGraceMs = int64(OrphanGrace / time.Millisecond)

// FreshnessWindow bounds how far a read may trail another process's
// commit: a read probes for the next record at most once per window per
// pointer, and only when a read asks (an idle night makes no requests).
const FreshnessWindow = 2 * time.Second

// envelopeMeta is the metadata key a blob's envelope rides under.
const envelopeMeta = "envelope"

// fetchWorkers bounds the parallel record reads when a log loads.
const fetchWorkers = 16

// Envelope is the typed metadata stored with every blob: the hash
// names the bytes, the envelope says how to read them.
type Envelope struct {
	Kind          string `json:"kind"` // network_document, edit_records, diversion_draft, manifest, plans, policy
	SchemaVersion uint32 `json:"schema_version"`
	ProducedBy    string `json:"produced_by"` // "cap-sync@sha256:<hex>", or "server" for host-owned documents
}

// LogEntry is one commit record in a pointer's append-only log.
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

// Head is a pointer's current value: its highest record. While the
// bucket does not answer (D-119) it is this process's cached head,
// Stale, as of the last time the bucket confirmed it.
type Head struct {
	Name     string   `json:"name"`
	Hash     string   `json:"hash"` // "" when the pointer does not exist
	Seq      int64    `json:"seq"`
	Envelope Envelope `json:"envelope"`
	AtMs     int64    `json:"at_ms"`           // the head record's commit time
	AsOfMs   int64    `json:"as_of_ms"`        // when the bucket last confirmed it is the head
	Stale    bool     `json:"stale,omitempty"` // the bucket did not answer this read
}

// ConflictError is a failed CAS: the pointer moved since the writer
// last saw it. Current is the hash to re-read and retry against.
type ConflictError struct {
	Pointer string
	Current string
	Seq     int64
}

func (e *ConflictError) Error() string {
	return fmt.Sprintf("store: pointer %q moved (current %s, seq %d)", e.Pointer, e.Current, e.Seq)
}

// ErrNotFound is a missing blob, pointer, or object.
var ErrNotFound = errors.New("store: not found")

// pointer is this process's view of one pointer: its head and the
// records it has read (records are immutable, so a read one is final).
type pointer struct {
	mu     sync.Mutex // serializes this process's refreshes and commits
	loaded bool
	seq    int64
	to     string
	env    Envelope
	atMs   int64
	recs   map[int64]LogEntry
	probed time.Time
}

// Store is one store over a backend. Safe for concurrent use, and for
// any number of processes over one bucket.
type Store struct {
	b       Backend
	serving bool
	cache   *blobCache
	mu      sync.Mutex
	ptrs    map[string]*pointer
	now     func() time.Time
	// afterList runs between a GC pass's listing and its deletes (tests:
	// the commit that lands there).
	afterList func()
}

// Open opens the store a URL names (OpenBackend) for a tool —
// `store gc`, `verify`, `copy` — with every operation the credentials
// allow.
func Open(url string) (*Store, error) {
	b, err := OpenBackend(context.Background(), url)
	if err != nil {
		return nil, err
	}
	return New(b), nil
}

// OpenServing opens the store a URL names for a serving process: it
// creates, reads, and lists, and never deletes (NewServing).
func OpenServing(url string) (*Store, error) {
	b, err := OpenBackend(context.Background(), url)
	if err != nil {
		return nil, err
	}
	return NewServing(b), nil
}

// New answers a store over a backend. Nothing is read until a pointer
// is asked for.
func New(b Backend) *Store {
	return &Store{b: guarded{b}, cache: newBlobCache(BlobCacheBytes), ptrs: map[string]*pointer{}, now: time.Now}
}

// NewServing answers the store a serving process holds (D-117): its
// backend refuses every delete and the store refuses GC, whatever the
// credentials allow — so a compromised server cannot erase history, and
// the bucket can carry the provider's immutability.
func NewServing(b Backend) *Store {
	s := New(servingBackend{b})
	s.serving = true
	return s
}

// ErrServingNeverDeletes is a delete asked of a serving store.
var ErrServingNeverDeletes = errors.New("store: a serving process never deletes (D-117; run `store gc`)")

type servingBackend struct{ Backend }

func (servingBackend) Delete(context.Context, string) error { return ErrServingNeverDeletes }

// Close releases nothing — a store holds no lock — and is kept so the
// server's shutdown stays one call.
func (s *Store) Close() error { return nil }

// ValidPointerName reports whether name may be a pointer: one key
// segment under pointers/ that cannot reach another pointer's records
// or hide as a dotfile — letters, digits, '.', '_' and '-', starting
// with a letter or digit, at most 128 bytes.
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

// ParseHash validates "sha256:<hex>" and returns the hex.
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

// HashOf is the store's content hash: "sha256:<hex>" of the bytes.
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

// recordSeq parses a record key's seq; ok is false for any other key.
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

// PutBlob stores data under its content hash with its envelope. It is
// idempotent: an existing blob is returned as-is (same key, same
// bytes). wantHash, when non-empty, is the module-produced document
// hash the caller asserts against sha256(data) — the store never mints
// bus-document identity.
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
		s.cache.put(hash, data, env)
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
	s.cache.put(hash, data, env)
	return hash, nil
}

// GetBlob reads a blob and its envelope.
func (s *Store) GetBlob(hash string) ([]byte, Envelope, error) {
	h, err := ParseHash(hash)
	if err != nil {
		return nil, Envelope{}, err
	}
	if c, ok := s.cache.get(hash); ok {
		return c.data, c.env, nil // immutable: a cached blob is always correct
	}
	data, meta, err := s.b.Get(context.Background(), blobKey(h))
	if err != nil {
		return nil, Envelope{}, err
	}
	var env Envelope
	if err := json.Unmarshal([]byte(meta[envelopeMeta]), &env); err != nil {
		return nil, Envelope{}, fmt.Errorf("blob %s envelope: %w", hash, err)
	}
	s.cache.put(hash, data, env)
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

// getRecord reads one record, from this process's cache or the bucket.
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

// loadRange reads records lo..hi into the cache, in parallel. Caller
// holds p.mu.
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

// advance moves the cached head to the highest record the bucket lists
// after it, checking that each new record chains from the one before.
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
	return nil
}

// fresh brings the cached head within the freshness window of the
// bucket: a first read lists the pointer; later, at most once per
// window, one Stat of record seq+1 (absent = the cache is current).
// Caller holds p.mu.
func (s *Store) fresh(p *pointer, name string) error {
	if !p.loaded {
		return s.advance(p, name)
	}
	if s.now().Sub(p.probed) < FreshnessWindow {
		return nil
	}
	if _, err := s.b.Stat(context.Background(), recordKey(name, p.seq+1)); errors.Is(err, ErrNotFound) {
		p.probed = s.now()
		return nil
	} else if err != nil {
		return err
	}
	return s.advance(p, name)
}

func (p *pointer) head(name string) Head {
	return Head{Name: name, Hash: p.to, Seq: p.seq, Envelope: p.env, AtMs: p.atMs, AsOfMs: p.probed.UnixMilli()}
}

// Head returns a pointer's current value. found is false when the
// pointer has no commits yet (or is no pointer name).
func (s *Store) Head(name string) (Head, bool, error) {
	if !ValidPointerName(name) {
		return Head{Name: name}, false, nil
	}
	p := s.pointer(name)
	p.mu.Lock()
	defer p.mu.Unlock()
	if err := s.fresh(p, name); err != nil {
		if !errors.Is(err, ErrUnavailable) || !p.loaded {
			return Head{Name: name}, false, err
		}
		// D-119: the bucket did not answer — the cached head, labelled.
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

// CAS commits data as the pointer's next value. from is the hash the
// writer last saw ("" = the pointer must not exist). On a moved
// pointer it returns *ConflictError with the current hash; the caller
// re-reads, re-merges (the kind's module op), and retries. The blob is
// created first, then record N+1 — created only if absent, which is the
// commit and the CAS at once — then the blob is read again and
// re-created if a GC pass removed it in between (store.tla, Check).
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
		// Idempotent redelivery: the same document is already the head —
		// no record, no seq bump (at-least-once converges to the same
		// hash AND the same seq).
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
	// Check: a commit that reused an aged orphan may have had it deleted
	// under it; the bytes are content-addressed, so the re-create is the
	// same object. A crash before this line is `store verify`'s to find.
	if _, err := s.b.Stat(context.Background(), blobKey(strings.TrimPrefix(hash, "sha256:"))); errors.Is(err, ErrNotFound) {
		if _, err := s.putBlob(data, env, ""); err != nil {
			return "", 0, fmt.Errorf("commit %s seq %d: re-create its blob: %w", name, entry.Seq, err)
		}
	} else if err != nil {
		return "", 0, fmt.Errorf("commit %s seq %d: re-read its blob: %w", name, entry.Seq, err)
	}
	return hash, entry.Seq, nil
}

// Log returns the records after the latest record whose To is since
// ("" = every record). found is false when since is not in the log —
// the caller then falls back to the whole current document (delta sync
// outside the log's history) — and when the pointer has no commits.
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

// Input reads inputs/<key>: an input an operator placed in the bucket
// for servers to boot from (D-118; `-feed bucket:<key>`). The store
// never writes there.
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

// InputModified answers inputs/<key>'s modified time — the cheap
// re-check before a read.
func (s *Store) InputModified(key string) (ObjectInfo, error) {
	return s.b.Stat(context.Background(), "inputs/"+key)
}
