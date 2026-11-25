package main

// A pick being published reads the stops document in cap-plans' `pick`
// op (planslog.go, writePick; D-237), and a stops write changes no pick
// (D-157). recordFeed is the server's only write to the document.

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
)

const (
	opLoadStops = 270
	opEvict     = 257
)

// recordFeed writes each difference between the feed's stops and the
// document as an event on the feed's first service date (D-150). It
// writes nothing without a store or a stops document, or when nothing
// differs.
func (s *server) recordFeed(ctx context.Context, feed *loadedFeed) error {
	l := s.keptLogOf("stops")
	if s.store == nil || l == nil {
		return nil
	}
	before := s.keptSeq("stops")
	op, _ := json.Marshal(map[string]any{"cap": "stops", "op": "import_events", "args": map[string]any{"network_hash": feed.networkHash, "feed": feed.hash}})
	w := logWrite{writer: "import:feed", op: "import_events", argsHash: feed.hash}
	seq, _, _, refusal, err := s.receiveLog(ctx, nil, l, w, receiveCall{ops: []json.RawMessage{op}, loads: []hostLoad{{opLoadNetwork, feed.document}}})
	if err != nil {
		return err
	}
	if refusal != nil {
		return fmt.Errorf("the recorded document is refused: %v", refusal.body)
	}
	if seq > before {
		log.Printf("stops: the feed %s recorded its changes in entry %d", feed.hash, seq)
	}
	return nil
}
