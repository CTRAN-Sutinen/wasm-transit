package main

// What the server reads of the stops document, and the one write it
// makes to it itself. The stops log is written through its receive op
// (logkinds.go; cap-stops `receive`), and the host decides nothing:
//
//   - a pick being published reads the stops in cap-plans' `pick` op
//     (planslog.go, writePick; D-237): the server registers the stops
//     document beside the pick. A stops write changes no pick (D-157).
//   - the agency's feed records its differences in the stops document
//     (`recordFeed`; D-150).

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

// recordFeed records the agency's feed in the stops document (D-150):
// each difference between the feed's stops and the document on the
// feed's first service date an event dated that day, naming the feed
// (cap-stops import_events with the feed's hash), written as one entry
// of the stops log like any write, the writer `import:feed`. The planner
// is shown what it did (the stops page's notice, cap-stops feed_events).
// Nothing without a store or a stops document, and nothing when the feed
// differs in nothing.
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
