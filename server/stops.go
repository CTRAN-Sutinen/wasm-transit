package main

// recordFeed is the server's only write to the kind the feed's
// differences go to (the kind table's feed_events).

import (
	"context"
	"encoding/json"
	"fmt"
	"log"

	"rustwasm/server/kinds"
	"rustwasm/server/logs"
	"rustwasm/server/wasmhost"
)

// recordFeed writes each difference between the feed and the kind's
// document as an event on the feed's first service date. It
// writes nothing without a store or such a document, or when nothing
// differs.
func (s *server) recordFeed(ctx context.Context, feed *loadedFeed) error {
	e := s.kindWhere(func(e *kinds.Entry) bool { return e.FeedEvents != "" })
	if s.store == nil || e == nil {
		return nil
	}
	l := s.kept().Of(e.StoreKind)
	if l == nil {
		return nil
	}
	before := s.kept().Seq(e.StoreKind)
	op, _ := json.Marshal(map[string]any{"cap": e.Module, "op": e.FeedEvents, "args": map[string]any{"network_hash": feed.networkHash, "feed": feed.hash}})
	w := logs.Write{Writer: "import:feed", Op: e.FeedEvents, ArgsHash: feed.hash}
	seq, _, _, refusal, err := s.kept().Receive(ctx, nil, l, w, logs.Call{Ops: []json.RawMessage{op}, Loads: []logs.Load{{Code: wasmhost.OpLoadNetwork, Doc: feed.document}}})
	if err != nil {
		return err
	}
	if refusal != nil {
		return fmt.Errorf("the recorded document is refused: %v", refusal.Body)
	}
	if seq > before {
		log.Printf("%s: the feed %s recorded its changes in entry %d", e.StoreKind, feed.hash, seq)
	}
	return nil
}
