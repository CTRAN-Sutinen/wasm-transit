package store

// The append-only logs (D-190; docs/FORMATS.md, "The logs"). A log is
// the record of one consistency domain. Each entry is one transaction.
// The current state is a fold of the entries, which the module that
// owns the log's kinds computes. This file only orders and keeps the
// entries. It treats them as opaque bytes.
//
//	logs/<log>/entries/<seq, 20 digits>.json      one entry
//	logs/<log>/checkpoints/<seq, 20 digits>.json  the fold up to seq
//
// An entry is committed by a create of seq+1 that lands only if the key
// is absent, the same CAS as a pointer record. After the create, the
// engine reads the entry back and compares the bytes, so a store that
// ignored the condition is caught at once (rule 6). A missing seq below
// the head is a gap. A gap stops writes to the log until an entry that
// names the cause closes it (rule 4).

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"strconv"
	"strings"
	"sync"
	"time"
)

// StoredEntry is one entry as the bucket holds it: the seq in its key,
// and its bytes.
type StoredEntry struct {
	Seq  int64  `json:"seq"`
	Text string `json:"text"`
}

// LogHead is a log's state as the bucket lists it.
type LogHead struct {
	Log string `json:"log"`
	// Seq is the highest entry listed (0: the log is empty).
	Seq int64 `json:"seq"`
	// Gaps are the missing seqs below Seq, in order. Writes stop while
	// there is one.
	Gaps []int64 `json:"gaps,omitempty"`
	// Checkpoint is the seq of the newest checkpoint (0: none).
	Checkpoint int64 `json:"checkpoint"`
	// AsOfMs is when the bucket last confirmed the head (Unix ms).
	AsOfMs int64 `json:"as_of_ms"`
}

// EntryConflictError is a lost race: another writer created the seq
// first. The caller reads the log again, runs its receive path again,
// and retries (rule 2).
type EntryConflictError struct {
	Log string
	Seq int64
}

func (e *EntryConflictError) Error() string {
	return fmt.Sprintf("store: log %q entry %d exists (another writer took it)", e.Log, e.Seq)
}

// GapError is a write to a log that has a gap. Only an entry at a
// missing seq is accepted until the gap closes.
type GapError struct {
	Log  string
	Gaps []int64
}

func (e *GapError) Error() string {
	return fmt.Sprintf("store: log %q has a gap at %v: writes stop until an entry that names the cause closes it", e.Log, e.Gaps)
}

// ErrOverwritten is an entry whose bytes, read back after its create,
// are not the bytes written: the store did not honor the conditional
// create. It is never a lost race. An operator must look at the store.
var ErrOverwritten = errors.New("store: an entry read back differs from the bytes written (the store ignored the create condition)")

// logState is this process's view of one log.
type logState struct {
	mu     sync.Mutex
	loaded bool
	head   LogHead
	probed time.Time
}

func logPrefix(name string) string         { return "logs/" + name + "/" }
func entriesPrefix(name string) string     { return logPrefix(name) + "entries/" }
func checkpointsPrefix(name string) string { return logPrefix(name) + "checkpoints/" }
func entryKey(name string, seq int64) string {
	return fmt.Sprintf("%s%020d.json", entriesPrefix(name), seq)
}
func checkpointKey(name string, seq int64) string {
	return fmt.Sprintf("%s%020d.json", checkpointsPrefix(name), seq)
}

// keySeq parses the seq of a key under prefix; ok is false for any
// other key.
func keySeq(prefix, key string) (int64, bool) {
	file, found := strings.CutPrefix(key, prefix)
	if !found {
		return 0, false
	}
	digits, isJSON := strings.CutSuffix(file, ".json")
	if !isJSON || len(digits) != 20 {
		return 0, false
	}
	seq, err := strconv.ParseInt(digits, 10, 64)
	if err != nil || seq < 1 {
		return 0, false
	}
	return seq, true
}

func (s *Store) logState(name string) *logState {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.logs == nil {
		s.logs = map[string]*logState{}
	}
	l, ok := s.logs[name]
	if !ok {
		l = &logState{head: LogHead{Log: name}}
		s.logs[name] = l
	}
	return l
}

// listLog reads the log's head from the bucket. A first read lists the
// checkpoints, then the entries after the newest one. A later read
// lists only the entries after the cached head. Caller holds l.mu.
func (s *Store) listLog(l *logState, name string) error {
	ctx := context.Background()
	head := l.head
	if !l.loaded {
		cps, err := s.b.List(ctx, checkpointsPrefix(name), "")
		if err != nil {
			return err
		}
		for _, o := range cps {
			if seq, ok := keySeq(checkpointsPrefix(name), o.Key); ok && seq > head.Checkpoint {
				head.Checkpoint = seq
			}
		}
		head.Seq = head.Checkpoint
		head.Gaps = nil
	}
	after := ""
	if head.Seq > 0 {
		after = entryKey(name, head.Seq)
	}
	objs, err := s.b.List(ctx, entriesPrefix(name), after)
	if err != nil {
		return err
	}
	prev := head.Seq
	for _, o := range objs {
		seq, ok := keySeq(entriesPrefix(name), o.Key)
		if !ok {
			continue
		}
		for missing := prev + 1; missing < seq; missing++ {
			head.Gaps = append(head.Gaps, missing)
		}
		prev = seq
	}
	head.Seq = prev
	// A gap closes when its entry appears: check each one still open.
	if len(head.Gaps) > 0 {
		open := head.Gaps[:0:0]
		for _, g := range head.Gaps {
			if _, err := s.b.Stat(ctx, entryKey(name, g)); errors.Is(err, ErrNotFound) {
				open = append(open, g)
			} else if err != nil {
				return err
			}
		}
		head.Gaps = open
	}
	l.head = head
	l.loaded = true
	l.probed = s.now()
	return nil
}

// freshLog brings the cached head within the freshness window: a first
// read lists the log; later, at most once per window, one Stat of the
// next entry. Caller holds l.mu.
func (s *Store) freshLog(l *logState, name string) error {
	if !l.loaded || len(l.head.Gaps) > 0 {
		return s.listLog(l, name)
	}
	if s.now().Sub(l.probed) < FreshnessWindow {
		return nil
	}
	if _, err := s.b.Stat(context.Background(), entryKey(name, l.head.Seq+1)); errors.Is(err, ErrNotFound) {
		l.probed = s.now()
		return nil
	} else if err != nil {
		return err
	}
	return s.listLog(l, name)
}

// LogHeadOf answers a log's head: its highest entry, its gaps, and its
// newest checkpoint.
func (s *Store) LogHeadOf(name string) (LogHead, error) {
	if !ValidPointerName(name) {
		return LogHead{Log: name}, fmt.Errorf("store: %q is not a log name", name)
	}
	l := s.logState(name)
	l.mu.Lock()
	defer l.mu.Unlock()
	if err := s.freshLog(l, name); err != nil {
		return LogHead{Log: name}, err
	}
	h := l.head
	h.Gaps = append([]int64(nil), h.Gaps...)
	h.AsOfMs = l.probed.UnixMilli()
	return h, nil
}

// AppendEntry creates the entry at seq, which must be the log's next
// seq, from a log with no gap. A lost race answers *EntryConflictError.
// After the create, the entry is read back and compared.
func (s *Store) AppendEntry(name string, seq int64, data []byte) error {
	return s.createEntry(name, seq, data, false)
}

// CloseGap creates the entry at a missing seq below the head: the entry
// that names why the seq was missing (rule 4). Any other seq is refused.
func (s *Store) CloseGap(name string, seq int64, data []byte) error {
	return s.createEntry(name, seq, data, true)
}

func (s *Store) createEntry(name string, seq int64, data []byte, gap bool) error {
	if !ValidPointerName(name) {
		return fmt.Errorf("store: %q is not a log name", name)
	}
	l := s.logState(name)
	l.mu.Lock()
	defer l.mu.Unlock()
	if err := s.listLog(l, name); err != nil {
		return err
	}
	if gap {
		isGap := false
		for _, g := range l.head.Gaps {
			isGap = isGap || g == seq
		}
		if !isGap {
			return fmt.Errorf("store: log %q has no gap at %d (gaps: %v)", name, seq, l.head.Gaps)
		}
	} else {
		if len(l.head.Gaps) > 0 {
			return &GapError{Log: name, Gaps: append([]int64(nil), l.head.Gaps...)}
		}
		if seq != l.head.Seq+1 {
			return &EntryConflictError{Log: name, Seq: seq}
		}
	}
	ctx := context.Background()
	key := entryKey(name, seq)
	if err := s.b.Create(ctx, key, data, nil); errors.Is(err, ErrExists) {
		_ = s.listLog(l, name)
		return &EntryConflictError{Log: name, Seq: seq}
	} else if err != nil {
		return err
	}
	got, _, err := s.b.Get(ctx, key)
	if err != nil {
		return fmt.Errorf("log %s entry %d: read back: %w", name, seq, err)
	}
	if !bytes.Equal(got, data) {
		return fmt.Errorf("log %s entry %d: %w", name, seq, ErrOverwritten)
	}
	return s.listLog(l, name)
}

// Entries reads the entries after seq `after`, up to `upto` (0: the
// head), in order. A missing entry in the range is a gap, and is
// skipped: the caller sees it in LogHead.Gaps.
func (s *Store) Entries(name string, after, upto int64) ([]StoredEntry, error) {
	if !ValidPointerName(name) {
		return nil, fmt.Errorf("store: %q is not a log name", name)
	}
	ctx := context.Background()
	start := ""
	if after > 0 {
		start = entryKey(name, after)
	}
	objs, err := s.b.List(ctx, entriesPrefix(name), start)
	if err != nil {
		return nil, err
	}
	var seqs []int64
	for _, o := range objs {
		if seq, ok := keySeq(entriesPrefix(name), o.Key); ok && seq > after && (upto == 0 || seq <= upto) {
			seqs = append(seqs, seq)
		}
	}
	out := make([]StoredEntry, len(seqs))
	errs := make([]error, len(seqs))
	var wg sync.WaitGroup
	next := make(chan int)
	for w := 0; w < fetchWorkers && w < len(seqs); w++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for i := range next {
				data, _, err := s.b.Get(ctx, entryKey(name, seqs[i]))
				out[i], errs[i] = StoredEntry{Seq: seqs[i], Text: string(data)}, err
			}
		}()
	}
	for i := range seqs {
		next <- i
	}
	close(next)
	wg.Wait()
	for i, err := range errs {
		if err != nil {
			return nil, fmt.Errorf("log %s entry %d: %w", name, seqs[i], err)
		}
	}
	return out, nil
}

// PutCheckpoint stores the fold at seq. It is create only: a checkpoint
// that exists with the same bytes is a success, and one with other
// bytes is an error, because two folds of the same entries must agree.
func (s *Store) PutCheckpoint(name string, seq int64, data []byte) error {
	if !ValidPointerName(name) {
		return fmt.Errorf("store: %q is not a log name", name)
	}
	ctx := context.Background()
	key := checkpointKey(name, seq)
	if err := s.b.Create(ctx, key, data, nil); err != nil && !errors.Is(err, ErrExists) {
		return err
	}
	got, _, err := s.b.Get(ctx, key)
	if err != nil {
		return err
	}
	if !bytes.Equal(got, data) {
		return fmt.Errorf("log %s checkpoint %d: the stored checkpoint differs from this fold", name, seq)
	}
	l := s.logState(name)
	l.mu.Lock()
	if seq > l.head.Checkpoint {
		l.head.Checkpoint = seq
	}
	l.mu.Unlock()
	return nil
}

// Checkpoints lists a log's checkpoint seqs, oldest first.
func (s *Store) Checkpoints(name string) ([]int64, error) {
	objs, err := s.b.List(context.Background(), checkpointsPrefix(name), "")
	if err != nil {
		return nil, err
	}
	var out []int64
	for _, o := range objs {
		if seq, ok := keySeq(checkpointsPrefix(name), o.Key); ok {
			out = append(out, seq)
		}
	}
	return out, nil
}

// Checkpoint reads the checkpoint at seq.
func (s *Store) Checkpoint(name string, seq int64) ([]byte, error) {
	data, _, err := s.b.Get(context.Background(), checkpointKey(name, seq))
	return data, err
}
