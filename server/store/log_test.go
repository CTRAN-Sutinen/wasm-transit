package store

// The laws of the append-only logs (D-190; log.go), over every backend
// (contract_test.go):
//
//  1. entries chain from 1 with no gap under racing writers, and a lost
//     race retries and lands;
//  2. a second process over the same bucket sees the entries;
//  3. a checkpoint is create only: the same bytes again succeed, other
//     bytes are refused, and a fresh process starts its listing there;
//  4. a gap stops writes, and an entry at the missing seq closes it;
//  5. the read-back after a create catches a store that overwrote the
//     entry.

import (
	"context"
	"errors"
	"fmt"
	"sync"
	"testing"
)

func entry(seq int64, who string) []byte {
	return []byte(fmt.Sprintf(`{"kind":"disruptions_entry","log":"disruptions","seq":%d,"writer":%q}`, seq, who))
}

// appendRetrying is a writer's loop: read the head, create the next
// entry, and on a lost race read again and retry (rule 2).
func appendRetrying(t *testing.T, s *Store, who string) int64 {
	for attempt := 0; attempt < 200; attempt++ {
		head, err := s.LogHeadOf("disruptions")
		if err != nil {
			t.Error(err)
			return 0
		}
		seq := head.Seq + 1
		err = s.AppendEntry("disruptions", seq, entry(seq, who))
		var ce *EntryConflictError
		if errors.As(err, &ce) {
			continue
		}
		if err != nil {
			t.Error(err)
			return 0
		}
		return seq
	}
	t.Errorf("%s: no seq after 200 attempts", who)
	return 0
}

func TestLogEntriesChainUnderRacingWriters(t *testing.T) {
	each(t, func(t *testing.T, b Backend, s *Store) {
		const writers, each = 6, 5
		other := New(b) // a second process over the same bucket
		var wg sync.WaitGroup
		got := make(chan int64, writers*each)
		for w := 0; w < writers; w++ {
			st := s
			if w%2 == 1 {
				st = other
			}
			wg.Add(1)
			go func(w int, st *Store) {
				defer wg.Done()
				for i := 0; i < each; i++ {
					got <- appendRetrying(t, st, fmt.Sprintf("w%d", w))
				}
			}(w, st)
		}
		wg.Wait()
		close(got)
		seen := map[int64]bool{}
		for seq := range got {
			if seen[seq] {
				t.Fatalf("seq %d landed twice", seq)
			}
			seen[seq] = true
		}
		head, err := New(b).LogHeadOf("disruptions")
		if err != nil || head.Seq != writers*each || len(head.Gaps) != 0 {
			t.Fatalf("head after the race: %+v %v", head, err)
		}
		entries, err := other.Entries("disruptions", 0, 0)
		if err != nil || len(entries) != writers*each {
			t.Fatalf("entries: %d %v", len(entries), err)
		}
		for i, e := range entries {
			if e.Seq != int64(i+1) || string(entry(e.Seq, ""))[:40] != e.Text[:40] {
				t.Fatalf("entry %d: %+v", i+1, e)
			}
		}
		if tail, _ := s.Entries("disruptions", 28, 0); len(tail) != 2 || tail[0].Seq != 29 {
			t.Fatalf("entries after 28: %+v", tail)
		}
		if mid, _ := s.Entries("disruptions", 3, 5); len(mid) != 2 || mid[1].Seq != 5 {
			t.Fatalf("entries 4..5: %+v", mid)
		}
	})
}

func TestALostRaceIsAConflictAndLeavesTheWinner(t *testing.T) {
	each(t, func(t *testing.T, b Backend, s *Store) {
		other := New(b)
		if _, err := s.LogHeadOf("disruptions"); err != nil { // s caches an empty head
			t.Fatal(err)
		}
		if err := other.AppendEntry("disruptions", 1, entry(1, "bob")); err != nil {
			t.Fatal(err)
		}
		var ce *EntryConflictError
		if err := s.AppendEntry("disruptions", 1, entry(1, "alice")); !errors.As(err, &ce) || ce.Seq != 1 {
			t.Fatalf("a create over a taken seq: %v", err)
		}
		if got, _ := s.Entries("disruptions", 0, 0); len(got) != 1 || got[0].Text != string(entry(1, "bob")) {
			t.Fatalf("the winner stands: %+v", got)
		}
		if seq := appendRetrying(t, s, "alice"); seq != 2 {
			t.Fatalf("the retry lands at 2, got %d", seq)
		}
		// A seq past the head is refused: entries never skip.
		if err := s.AppendEntry("disruptions", 9, entry(9, "x")); !errors.As(err, &ce) {
			t.Fatalf("a seq past the head: %v", err)
		}
	})
}

func TestACheckpointIsCreateOnlyAndStartsTheListing(t *testing.T) {
	each(t, func(t *testing.T, b Backend, s *Store) {
		for i := 0; i < 4; i++ {
			appendRetrying(t, s, "w")
		}
		fold := []byte(`{"kind":"disruptions_checkpoint","seq":3}`)
		if err := s.PutCheckpoint("disruptions", 3, fold); err != nil {
			t.Fatal(err)
		}
		if err := New(b).PutCheckpoint("disruptions", 3, fold); err != nil {
			t.Fatalf("the same fold again: %v", err)
		}
		if err := s.PutCheckpoint("disruptions", 3, []byte(`{"other":true}`)); err == nil {
			t.Fatal("a different fold at the same seq must be refused")
		}
		fresh := New(b)
		head, err := fresh.LogHeadOf("disruptions")
		if err != nil || head.Seq != 4 || head.Checkpoint != 3 {
			t.Fatalf("a fresh process: %+v %v", head, err)
		}
		if cps, _ := fresh.Checkpoints("disruptions"); len(cps) != 1 || cps[0] != 3 {
			t.Fatalf("checkpoints: %v", cps)
		}
		if got, err := fresh.Checkpoint("disruptions", 3); err != nil || string(got) != string(fold) {
			t.Fatalf("checkpoint 3: %q %v", got, err)
		}
	})
}

func TestAGapStopsWritesUntilAnEntryClosesIt(t *testing.T) {
	each(t, func(t *testing.T, b Backend, s *Store) {
		for i := 0; i < 3; i++ {
			appendRetrying(t, s, "w")
		}
		// Someone deletes entry 2 by hand.
		if err := b.Delete(context.Background(), entryKey("disruptions", 2)); err != nil {
			t.Fatal(err)
		}
		fresh := New(b)
		head, err := fresh.LogHeadOf("disruptions")
		if err != nil || head.Seq != 3 || len(head.Gaps) != 1 || head.Gaps[0] != 2 {
			t.Fatalf("the gap: %+v %v", head, err)
		}
		var ge *GapError
		if err := fresh.AppendEntry("disruptions", 4, entry(4, "w")); !errors.As(err, &ge) {
			t.Fatalf("a write over a gap: %v", err)
		}
		if err := fresh.CloseGap("disruptions", 3, entry(3, "w")); err == nil {
			t.Fatal("CloseGap at a seq that is no gap must be refused")
		}
		if err := fresh.CloseGap("disruptions", 2, []byte(`{"intent":{"op":"close_gap","note":"deleted by hand"}}`)); err != nil {
			t.Fatal(err)
		}
		if seq := appendRetrying(t, fresh, "w"); seq != 4 {
			t.Fatalf("writes go on at 4, got %d", seq)
		}
	})
}

// overwriting is a store that ignores the create condition: every
// read of an entry answers other bytes.
type overwriting struct{ Backend }

func (o overwriting) Get(ctx context.Context, key string) ([]byte, map[string]string, error) {
	data, meta, err := o.Backend.Get(ctx, key)
	if err == nil && len(data) > 0 && key == entryKey("disruptions", 1) {
		data = append([]byte(nil), data...)
		data[len(data)-2] ^= 1
	}
	return data, meta, err
}

func TestTheReadBackCatchesAnOverwrittenEntry(t *testing.T) {
	s := New(overwriting{NewMemory()})
	if err := s.AppendEntry("disruptions", 1, entry(1, "w")); !errors.Is(err, ErrOverwritten) {
		t.Fatalf("the read-back: %v", err)
	}
}
