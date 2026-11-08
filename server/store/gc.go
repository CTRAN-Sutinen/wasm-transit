package store

import (
	"context"
	"errors"
	"strings"
)

// GCReport is what one GC pass did.
type GCReport struct {
	Removed  int `json:"removed"`  // orphan blobs this pass deleted
	Restored int `json:"restored"` // trashed blobs a record named by the time the pass settled
	Kept     int `json:"kept"`     // candidates whose delete the bucket refused (retention)
}

// GC is the offline collector (D-117; `store gc`), run beside live
// servers with its own delete-capable credentials. A pass (store.tla):
//
//  1. settles any trash a crashed pass left (step 4);
//  2. lists every record, then the blobs: the candidates are the blobs
//     no record names (neither `to` nor `from`) older than graceMs;
//  3. moves each candidate to gc/<hex> — a copy, then the delete;
//  4. settles: lists the records again, restores every trashed blob a
//     record now names (a commit that reused the orphan after step 2),
//     and empties gc/.
//
// Step 4 closes the race a commit's own re-check cannot: a commit that
// lands its record and re-reads its blob between steps 2 and 3. A GC
// pass never deletes a record.
func (s *Store) GC(graceMs int64) (GCReport, error) {
	var r GCReport
	if s.serving {
		return r, ErrServingNeverDeletes
	}
	if err := s.settle(&r, nil); err != nil {
		return r, err
	}
	named, err := s.named()
	if err != nil {
		return r, err
	}
	if s.afterList != nil {
		s.afterList()
	}
	ctx := context.Background()
	blobs, err := s.b.List(ctx, "blobs/", "")
	if err != nil {
		return r, err
	}
	now := s.now()
	deleted := map[string]bool{}
	for _, o := range blobs {
		h := strings.TrimPrefix(o.Key, "blobs/")
		if named[h] || now.Sub(o.Modified).Milliseconds() < graceMs {
			continue
		}
		data, meta, err := s.b.Get(ctx, o.Key)
		if errors.Is(err, ErrNotFound) {
			continue
		} else if err != nil {
			return r, err
		}
		if err := s.b.Create(ctx, trashKey(h), data, meta); err != nil && !errors.Is(err, ErrExists) {
			return r, err
		}
		if err := s.b.Delete(ctx, o.Key); err != nil {
			r.Kept++ // under retention: it stays, and settle drops the copy
			continue
		}
		deleted[h] = true
		s.cache.drop("sha256:" + h)
	}
	return r, s.settle(&r, deleted)
}

func trashKey(h string) string { return "gc/" + h }

// settle re-lists the records, restores every blob under gc/ a record
// names, and deletes gc/. deleted is what this pass removed from blobs/
// (nil when settling a crashed pass's trash).
func (s *Store) settle(r *GCReport, deleted map[string]bool) error {
	ctx := context.Background()
	trash, err := s.b.List(ctx, "gc/", "")
	if err != nil || len(trash) == 0 {
		return err
	}
	named, err := s.named()
	if err != nil {
		return err
	}
	for _, o := range trash {
		h := strings.TrimPrefix(o.Key, "gc/")
		if named[h] {
			data, meta, err := s.b.Get(ctx, o.Key)
			if err != nil {
				return err
			}
			switch err := s.b.Create(ctx, blobKey(h), data, meta); {
			case err == nil:
				r.Restored++
			case !errors.Is(err, ErrExists):
				return err
			}
		} else if deleted[h] {
			r.Removed++
		}
		if err := s.b.Delete(ctx, o.Key); err != nil {
			return err
		}
	}
	return nil
}

// named answers the hex of every blob a record names (`to` or `from`),
// over every pointer in the bucket — another process's commits too.
func (s *Store) named() (map[string]bool, error) {
	recs, err := s.b.List(context.Background(), "pointers/", "")
	if err != nil {
		return nil, err
	}
	top := map[string]int64{}
	for _, o := range recs {
		if name, seq, ok := recordSeq(o.Key); ok && seq > top[name] {
			top[name] = seq
		}
	}
	named := map[string]bool{}
	for name, hi := range top {
		p := s.pointer(name)
		p.mu.Lock()
		err := s.loadRange(p, name, 1, hi)
		for seq := int64(1); err == nil && seq <= hi; seq++ {
			e := p.recs[seq]
			for _, h := range []string{e.To, e.From} {
				if x, perr := ParseHash(h); perr == nil {
					named[x] = true
				}
			}
		}
		p.mu.Unlock()
		if err != nil {
			return nil, err
		}
	}
	return named, nil
}
