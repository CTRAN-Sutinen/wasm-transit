package main

// recordFeed is the server's only write to the kind the feed's
// differences go to (the kind table's feed_events).

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
)

const opEvict = 257

// recordFeed writes each difference between the feed and the kind's
// document as an event on the feed's first service date (D-150). It
// writes nothing without a store or such a document, or when nothing
// differs.
func (s *server) recordFeed(ctx context.Context, feed *loadedFeed) error {
	e := s.kindWhere(func(e *kindEntry) bool { return e.FeedEvents != "" })
	if s.store == nil || e == nil {
		return nil
	}
	l := s.keptLogOf(e.StoreKind)
	if l == nil {
		return nil
	}
	before := s.keptSeq(e.StoreKind)
	op, _ := json.Marshal(map[string]any{"cap": e.Module, "op": e.FeedEvents, "args": map[string]any{"network_hash": feed.networkHash, "feed": feed.hash}})
	w := logWrite{writer: "import:feed", op: e.FeedEvents, argsHash: feed.hash}
	seq, _, _, refusal, err := s.receiveLog(ctx, nil, l, w, receiveCall{ops: []json.RawMessage{op}, loads: []hostLoad{{opLoadNetwork, feed.document}}})
	if err != nil {
		return err
	}
	if refusal != nil {
		return fmt.Errorf("the recorded document is refused: %v", refusal.body)
	}
	if seq > before {
		log.Printf("%s: the feed %s recorded its changes in entry %d", e.StoreKind, feed.hash, seq)
	}
	return nil
}
