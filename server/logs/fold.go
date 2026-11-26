package logs

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"sync"

	"rustwasm/server/store"
	"rustwasm/server/wasmhost"
)

// The fold: cap-log folds a log's entries from its newest checkpoint
// that checks, and the host stores a checkpoint every CheckpointEvery
// entries. A gap stops the fold until close-gap names its cause.

// CheckpointEvery: a boot folds at most this many entries after the newest
// checkpoint.
const CheckpointEvery = 256

func (ls *Logs) logModule() (*wasmhost.Host, error) {
	return ls.long().As("log"), nil
}

func (ls *Logs) logOp(req map[string]any, out any) error {
	m, err := ls.logModule()
	if err != nil {
		return fmt.Errorf("the log module: %w", err)
	}
	return m.Invoke(context.Background(), req, out)
}

type fold struct {
	name     string
	assemble func(records map[string]json.RawMessage) error
	// revise is set for a kind whose merge would undo a revert (D-193).
	// It answers the records that the revert writes back (cap-log
	// `written`) in place of the states before.
	revise func(states json.RawMessage, nowMs int64) (json.RawMessage, error)
	// documents names the hashes of the documents the fold serves.
	// The caller holds mu.
	documents func() map[string]string

	mu     sync.Mutex
	loaded bool
	base   json.RawMessage
	seq    int64
	// stored is the seq of the newest checkpoint in the bucket.
	stored int64
	asOfMs int64
	// stale means the bucket did not answer, so the head is the last one
	// that this process or its disk cache confirmed.
	stale       bool
	quarantined map[int64]string
	gaps        []int64
}

type logFoldAnswer struct {
	Seq         int64                      `json:"seq"`
	Checkpoint  string                     `json:"checkpoint"`
	Records     map[string]json.RawMessage `json:"records"`
	Quarantined []struct {
		Seq    int64  `json:"seq"`
		Reason string `json:"reason"`
	} `json:"quarantined"`
}

func (ls *Logs) foldLog(name string, checkpoint json.RawMessage, entries []store.StoredEntry) (logFoldAnswer, error) {
	if entries == nil {
		entries = []store.StoredEntry{}
	}
	req := map[string]any{"op": "fold", "log": name, "entries": entries}
	if len(checkpoint) > 0 {
		req["checkpoint"] = checkpoint
	}
	var ans logFoldAnswer
	err := ls.logOp(req, &ans)
	return ans, err
}

// bootLogLocked starts from the newest checkpoint that equals the fold
// from the checkpoint before it, or from the empty log. A checkpoint
// that differs raises an alarm. The caller holds f.mu.
func (ls *Logs) bootLogLocked(f *fold, upto int64) error {
	cps, err := ls.store.Checkpoints(f.name)
	if err != nil {
		return err
	}
	f.base, f.seq, f.stored = nil, 0, 0
	for i := len(cps) - 1; i >= 0; i-- {
		at := cps[i]
		if at > upto {
			continue
		}
		var prev json.RawMessage
		var prevSeq int64
		if i > 0 {
			prevSeq = cps[i-1]
			if prev, err = ls.store.Checkpoint(f.name, prevSeq); err != nil {
				return err
			}
		}
		stored, err := ls.store.Checkpoint(f.name, at)
		if err != nil {
			return err
		}
		entries, err := ls.store.Entries(f.name, prevSeq, at)
		if err != nil {
			return err
		}
		ans, err := ls.foldLog(f.name, prev, entries)
		if err == nil && ans.Seq == at && ans.Checkpoint == string(stored) {
			f.base, f.seq, f.stored = json.RawMessage(stored), at, at
			return nil
		}
		log.Printf("ALARM %s log: checkpoint %d differs from the fold of the entries before it (%v): the boot starts from an older one", f.name, at, err)
	}
	return nil
}

// refreshLogLocked folds only the entries up to the first gap. The
// caller holds f.mu.
func (ls *Logs) refreshLogLocked(f *fold) error {
	head, err := ls.store.LogHeadOf(f.name)
	if err != nil {
		return err
	}
	upto := head.Seq
	if len(head.Gaps) > 0 {
		upto = head.Gaps[0] - 1
		if len(f.gaps) == 0 || f.gaps[0] != head.Gaps[0] {
			log.Printf("ALARM %s log: a gap at %v: writes stop until POST /logs/%s/close-gap names the cause", f.name, head.Gaps, f.name)
		}
	}
	f.gaps = head.Gaps
	if !f.loaded {
		if err := ls.bootLogLocked(f, upto); err != nil {
			return err
		}
		f.quarantined = map[int64]string{}
	}
	if upto > f.seq || !f.loaded {
		entries, err := ls.store.Entries(f.name, f.seq, upto)
		if err != nil {
			return err
		}
		ans, err := ls.foldLog(f.name, f.base, entries)
		if err != nil {
			return fmt.Errorf("fold the %s log: %w", f.name, err)
		}
		for _, q := range ans.Quarantined {
			if _, seen := f.quarantined[q.Seq]; !seen {
				log.Printf("ALARM %s log: entry %d is quarantined: %s", f.name, q.Seq, q.Reason)
			}
			f.quarantined[q.Seq] = q.Reason
		}
		if err := f.assemble(ans.Records); err != nil {
			return fmt.Errorf("the %s log's documents: %w", f.name, err)
		}
		f.base, f.seq = json.RawMessage(ans.Checkpoint), ans.Seq
		f.loaded = true
		if !head.Stale && f.seq-f.stored >= CheckpointEvery {
			if err := ls.store.PutCheckpoint(f.name, f.seq, f.base); err != nil {
				log.Printf("ALARM %s log: checkpoint %d: %v", f.name, f.seq, err)
			} else {
				f.stored = f.seq
			}
		}
	}
	f.asOfMs, f.stale = head.AsOfMs, head.Stale
	return nil
}

func errStaleHead(f *fold) error {
	return fmt.Errorf("%w: the %s log's head is as of %d", store.ErrUnavailable, f.name, f.asOfMs)
}

// refreshLog keeps a loaded fold, and reports it stale, while the bucket
// does not answer.
func (ls *Logs) refreshLog(f *fold) (stale bool, err error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	err = ls.refreshLogLocked(f)
	if err != nil && errors.Is(err, store.ErrUnavailable) && f.loaded {
		return true, nil
	}
	return err == nil && f.stale, err
}

// foldAt reads the store, never another log's fold, so a write that
// holds its own log's lock can call it.
func (ls *Logs) foldAt(name string, seq int64) (json.RawMessage, error) {
	if seq <= 0 {
		return json.RawMessage("{}"), nil
	}
	cps, err := ls.store.Checkpoints(name)
	if err != nil {
		return nil, err
	}
	var from int64
	for _, c := range cps {
		if c <= seq && c > from {
			from = c
		}
	}
	req := map[string]any{"op": "fold", "log": name}
	if from > 0 {
		cp, err := ls.store.Checkpoint(name, from)
		if err != nil {
			return nil, err
		}
		req["checkpoint"] = json.RawMessage(cp)
	}
	entries, err := ls.store.Entries(name, from, seq)
	if err != nil {
		return nil, err
	}
	req["entries"] = entries
	var got struct {
		Records json.RawMessage `json:"records"`
	}
	if err := ls.logOp(req, &got); err != nil {
		return nil, err
	}
	return got.Records, nil
}

// Checkpointed is the seq of the newest checkpoint in the bucket that the
// fold holds.
func (l *Log) Checkpointed() int64 {
	l.fold.mu.Lock()
	defer l.fold.mu.Unlock()
	return l.fold.stored
}
