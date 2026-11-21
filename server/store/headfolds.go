package store

import (
	"context"
	"strings"
)

// HeadFolds folds each log under logs/ to its head through fold: its
// newest checkpoint and the entries after it. The answer is each log's
// fold at its head (the checkpoint's bytes), and its seq. An empty log
// is left out. The existence check reads them (D-233; `store revert`).
func HeadFolds(ctx context.Context, b Backend, fold LogFolder) (map[string][]byte, map[string]int64, error) {
	s := New(b)
	objs, err := b.List(ctx, "logs/", "")
	if err != nil {
		return nil, nil, err
	}
	heads, seqs := map[string][]byte{}, map[string]int64{}
	seen := map[string]bool{}
	for _, o := range objs {
		name, _, _ := strings.Cut(strings.TrimPrefix(o.Key, "logs/"), "/")
		if seen[name] || !ValidPointerName(name) {
			continue
		}
		seen[name] = true
		head, err := s.LogHeadOf(name)
		if err != nil {
			return nil, nil, err
		}
		if head.Seq == 0 {
			continue
		}
		var base []byte
		if head.Checkpoint > 0 {
			if base, err = s.Checkpoint(name, head.Checkpoint); err != nil {
				return nil, nil, err
			}
		}
		entries, err := s.Entries(name, head.Checkpoint, head.Seq)
		if err != nil {
			return nil, nil, err
		}
		folded, err := fold(name, base, entries)
		if err != nil {
			return nil, nil, err
		}
		heads[name], seqs[name] = folded, head.Seq
	}
	return heads, seqs, nil
}
