package store

// The append-only logs (D-190; docs/FORMATS.md, "The logs"). The store
// treats entries as opaque bytes.
//
//	logs/<log>/entries/<seq, 20 digits>.json      one entry
//	logs/<log>/checkpoints/<seq, 20 digits>.json  the fold up to seq
//
// An entry commits by a create of seq+1 that lands only if the key is
// absent. The store then reads the entry back and compares the bytes, so
// a store that ignored the condition is caught at once (rule 6). A
// missing seq below the head is a gap. It stops writes until an entry
// that names the cause closes it (rule 4).

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

type StoredEntry struct {
	Seq  int64  `json:"seq"`
	Text string `json:"text"`
}

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
	// Stale means the bucket did not answer this read, so the head is the
	// one that this process or its disk cache held at AsOfMs (D-119).
	Stale bool `json:"stale,omitempty"`
}

// EntryConflictError means another writer created the seq first. The
// caller reads the log again and retries (rule 2).
type EntryConflictError struct {
	Log string
	Seq int64
}

func (e *EntryConflictError) Error() string {
	return fmt.Sprintf("store: log %q entry %d exists (another writer took it)", e.Log, e.Seq)
}

type GapError struct {
	Log  string
	Gaps []int64
}

func (e *GapError) Error() string {
	return fmt.Sprintf("store: log %q has a gap at %v: writes stop until an entry that names the cause closes it", e.Log, e.Gaps)
}

// ErrOverwritten means the store did not honor the conditional create.
// It is never a lost race, and an operator must look at the store.
var ErrOverwritten = errors.New("store: an entry read back differs from the bytes written (the store ignored the create condition)")

type logState struct {
	mu     sync.Mutex
	loaded bool
	head   LogHead
	probed time.Time
	// fromCache means the head is the disk cache's. The next list that
	// the bucket answers starts again from the checkpoints.
	fromCache bool
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

// listLog lists the checkpoints on a first read, then the entries after
// the newest one. Later reads list only the entries after the cached
// head. The caller holds l.mu.
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
	s.keepLogHead(l, name)
	return nil
}

func (s *Store) keepLogHead(l *logState, name string) {
	if s.disk == nil {
		return
	}
	h := l.head
	h.Gaps = append([]int64(nil), h.Gaps...)
	h.AsOfMs = l.probed.UnixMilli()
	s.disk.putLogHead(name, h)
}

// relistLog keeps the cache's head on a failure.
func (s *Store) relistLog(l *logState, name string) error {
	saved, probed := l.head, l.probed
	l.head, l.loaded = LogHead{Log: name}, false
	if err := s.listLog(l, name); err != nil {
		l.head, l.probed, l.loaded = saved, probed, true
		return err
	}
	l.fromCache = false
	return nil
}

func (s *Store) listFresh(l *logState, name string) error {
	if l.fromCache {
		return s.relistLog(l, name)
	}
	return s.listLog(l, name)
}

// freshLog does one Stat of the next entry at most once per
// FreshnessWindow. The caller holds l.mu.
func (s *Store) freshLog(l *logState, name string) error {
	if !l.loaded || l.fromCache || len(l.head.Gaps) > 0 {
		return s.listFresh(l, name)
	}
	if s.now().Sub(l.probed) < FreshnessWindow {
		return nil
	}
	if _, err := s.b.Stat(context.Background(), entryKey(name, l.head.Seq+1)); errors.Is(err, ErrNotFound) {
		l.probed = s.now()
		s.keepLogHead(l, name)
		return nil
	} else if err != nil {
		return err
	}
	return s.listLog(l, name)
}

// LogHeadOf answers the head this process last confirmed, else the disk
// cache's, marked Stale, while the bucket does not answer. With neither,
// it answers the error.
func (s *Store) LogHeadOf(name string) (LogHead, error) {
	if !ValidPointerName(name) {
		return LogHead{Log: name}, fmt.Errorf("store: %q is not a log name", name)
	}
	l := s.logState(name)
	l.mu.Lock()
	defer l.mu.Unlock()
	err := s.freshLog(l, name)
	if err != nil && errors.Is(err, ErrUnavailable) && !l.loaded {
		if h, ok := s.disk.logHead(name); ok {
			h.Log, h.Stale = name, false
			l.head, l.probed, l.loaded, l.fromCache = h, time.UnixMilli(h.AsOfMs), true, true
		}
	}
	if err != nil && (!errors.Is(err, ErrUnavailable) || !l.loaded) {
		return LogHead{Log: name}, err
	}
	h := l.head
	h.Stale = err != nil
	h.Gaps = append([]int64(nil), h.Gaps...)
	h.AsOfMs = l.probed.UnixMilli()
	return h, nil
}

// AppendEntry needs seq to be the next seq of a log with no gap. A lost
// race answers *EntryConflictError.
func (s *Store) AppendEntry(name string, seq int64, data []byte) error {
	return s.createEntry(name, seq, data, false)
}

// CloseGap refuses any seq except a missing one below the head (rule 4).
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
	if err := s.listFresh(l, name); err != nil {
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
	s.disk.putLogObject(name, "entries", seq, data)
	return s.listLog(l, name)
}

// Entries reads the entries after `after`, up to `upto` (0 is the head).
// It skips a gap, which LogHead.Gaps names. While the bucket does not
// answer, the disk cache answers when it holds every entry in the range.
func (s *Store) Entries(name string, after, upto int64) ([]StoredEntry, error) {
	if !ValidPointerName(name) {
		return nil, fmt.Errorf("store: %q is not a log name", name)
	}
	out, err := s.entries(name, after, upto)
	if errors.Is(err, ErrUnavailable) {
		if cached, ok := s.cachedEntries(name, after, upto); ok {
			return cached, nil
		}
	}
	if err != nil {
		return nil, err
	}
	for _, e := range out {
		s.disk.putLogObject(name, "entries", e.Seq, []byte(e.Text))
	}
	return out, nil
}

func (s *Store) cachedEntries(name string, after, upto int64) ([]StoredEntry, bool) {
	head, ok := s.disk.logHead(name)
	if !ok {
		return nil, false
	}
	if upto == 0 || upto > head.Seq {
		upto = head.Seq
	}
	gap := map[int64]bool{}
	for _, g := range head.Gaps {
		gap[g] = true
	}
	out := []StoredEntry{}
	for seq := after + 1; seq <= upto; seq++ {
		if gap[seq] {
			continue
		}
		data, ok := s.disk.logObject(name, "entries", seq)
		if !ok {
			return nil, false
		}
		out = append(out, StoredEntry{Seq: seq, Text: string(data)})
	}
	return out, true
}

func (s *Store) entries(name string, after, upto int64) ([]StoredEntry, error) {
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

// PutCheckpoint is create only. A checkpoint that exists with other bytes
// is an error, because two folds of the same entries must agree.
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
	s.disk.putLogObject(name, "checkpoints", seq, data)
	l := s.logState(name)
	l.mu.Lock()
	if seq > l.head.Checkpoint {
		l.head.Checkpoint = seq
		if l.loaded && !l.fromCache {
			s.keepLogHead(l, name)
		}
	}
	l.mu.Unlock()
	return nil
}

func (s *Store) Checkpoints(name string) ([]int64, error) {
	objs, err := s.b.List(context.Background(), checkpointsPrefix(name), "")
	if errors.Is(err, ErrUnavailable) && s.disk != nil {
		if _, ok := s.disk.logHead(name); ok {
			return s.disk.logSeqs(name, "checkpoints"), nil
		}
	}
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

func (s *Store) Checkpoint(name string, seq int64) ([]byte, error) {
	data, _, err := s.b.Get(context.Background(), checkpointKey(name, seq))
	if errors.Is(err, ErrUnavailable) {
		if cached, ok := s.disk.logObject(name, "checkpoints", seq); ok {
			return cached, nil
		}
	}
	if err == nil {
		s.disk.putLogObject(name, "checkpoints", seq, data)
	}
	return data, err
}
