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
// number of processes may share one bucket. A crash between the two
// orphans the blob; GC deletes blobs no record names, past the grace
// period. Nothing is ever overwritten, appended, renamed, or locked.
// The store never mints document identity for bus documents: callers
// pass the module-produced hash and the store asserts sha256(bytes) ==
// hash, failing loudly on mismatch.
//
// The invariants: every pointer, current or historical, names a
// durable blob; the log is append-only and each record's `from` is the
// prior `to`; GC never deletes a referenced blob; a failed CAS leaves
// authority untouched. store.tla (`just spec`) still models the
// single-process append engine; its object model is open work
// (docs/units/store-durability.md).
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

// OrphanGraceMs is how long an unreferenced blob survives before GC
// collects it — long enough that a crash mid-commit is never data
// loss, short enough that orphans do not pile up.
const OrphanGraceMs = 24 * 3600 * 1000

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

// Head is a pointer's current value: its highest record.
type Head struct {
	Name     string   `json:"name"`
	Hash     string   `json:"hash"` // "" when the pointer does not exist
	Seq      int64    `json:"seq"`
	Envelope Envelope `json:"envelope"`
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
	recs   map[int64]LogEntry
	probed time.Time
}

// Store is one store over a backend. Safe for concurrent use, and for
// any number of processes over one bucket.
type Store struct {
	b    Backend
	mu   sync.Mutex
	ptrs map[string]*pointer
	// gc excludes a GC pass from a commit in this process, so a pass
	// never deletes an aged orphan a commit is about to name.
	gc  sync.RWMutex
	now func() time.Time
}

// Open opens the store a URL names (OpenBackend): `mem://`, or
// `s3://bucket/prefix?endpoint=…`.
func Open(url string) (*Store, error) {
	b, err := OpenBackend(context.Background(), url)
	if err != nil {
		return nil, err
	}
	return New(b), nil
}

// New answers a store over a backend. Nothing is read until a pointer
// is asked for.
func New(b Backend) *Store {
	return &Store{b: b, ptrs: map[string]*pointer{}, now: time.Now}
}

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
	s.gc.RLock()
	defer s.gc.RUnlock()
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
	return hash, nil
}

// GetBlob reads a blob and its envelope.
func (s *Store) GetBlob(hash string) ([]byte, Envelope, error) {
	h, err := ParseHash(hash)
	if err != nil {
		return nil, Envelope{}, err
	}
	data, meta, err := s.b.Get(context.Background(), blobKey(h))
	if err != nil {
		return nil, Envelope{}, err
	}
	var env Envelope
	if err := json.Unmarshal([]byte(meta[envelopeMeta]), &env); err != nil {
		return nil, Envelope{}, fmt.Errorf("blob %s envelope: %w", hash, err)
	}
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
		p.seq, p.to = top, head.To
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
	return Head{Name: name, Hash: p.to, Seq: p.seq, Envelope: p.env}
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
		return Head{Name: name}, false, err
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
// commit and the CAS at once.
func (s *Store) CAS(name, from string, data []byte, env Envelope, wantHash, writer string, atMs int64) (string, int64, error) {
	if !ValidPointerName(name) {
		return "", 0, fmt.Errorf("store: %q is not a pointer name", name)
	}
	hash := hashOf(data)
	if wantHash != "" && wantHash != hash {
		return "", 0, fmt.Errorf("hash mismatch: module says %s, bytes are %s", wantHash, hash)
	}
	// The GC lock before the pointer's: a pass takes them in that order.
	s.gc.RLock()
	defer s.gc.RUnlock()
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
	p.seq, p.to, p.env = entry.Seq, hash, env
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

// GC deletes blobs no record names (neither `to` nor `from`) and older
// than graceMs — the crash-torn orphans. It never touches a referenced
// blob, and never deletes a record. It reads every record in the
// bucket, so a commit another process made is counted.
func (s *Store) GC(graceMs int64) (int, error) {
	s.gc.Lock()
	defer s.gc.Unlock()
	ctx := context.Background()
	recs, err := s.b.List(ctx, "pointers/", "")
	if err != nil {
		return 0, err
	}
	top := map[string]int64{}
	for _, o := range recs {
		if name, seq, ok := recordSeq(o.Key); ok && seq > top[name] {
			top[name] = seq
		}
	}
	referenced := map[string]bool{}
	for name, hi := range top {
		p := s.pointer(name)
		p.mu.Lock()
		err := s.loadRange(p, name, 1, hi)
		for seq := int64(1); err == nil && seq <= hi; seq++ {
			e := p.recs[seq]
			for _, h := range []string{e.To, e.From} {
				if x, perr := ParseHash(h); perr == nil {
					referenced[x] = true
				}
			}
		}
		p.mu.Unlock()
		if err != nil {
			return 0, err
		}
	}
	blobs, err := s.b.List(ctx, "blobs/", "")
	if err != nil {
		return 0, err
	}
	now := s.now()
	removed := 0
	for _, o := range blobs {
		if referenced[strings.TrimPrefix(o.Key, "blobs/")] {
			continue
		}
		if now.Sub(o.Modified).Milliseconds() < graceMs {
			continue
		}
		if err := s.b.Delete(ctx, o.Key); err != nil {
			return removed, err
		}
		removed++
	}
	return removed, nil
}

// Sweep runs GC every interval in the background until the returned
// stop is called (stop is idempotent and waits for a running pass).
// A pass excludes this process's commits, so it never lands between a
// commit's blob and its record.
func (s *Store) Sweep(interval time.Duration, graceMs int64) (stop func()) {
	done := make(chan struct{})
	finished := make(chan struct{})
	go func() {
		defer close(finished)
		ticker := time.NewTicker(interval)
		defer ticker.Stop()
		for {
			select {
			case <-done:
				return
			case <-ticker.C:
				s.GC(graceMs)
			}
		}
	}()
	var once sync.Once
	return func() {
		once.Do(func() { close(done) })
		<-finished
	}
}
