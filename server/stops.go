package main

// What the server reads of the stops document, and the one write it
// makes to it itself. The stops log is written through its receive op
// (logkinds.go; cap-stops `receive`), and the host decides nothing:
//
//   - a pick being published reads the stops (`readPick`): each stop's
//     record on the pick's first date, and the warnings of the stops it
//     visits that are not in service over its period, as one cap-stops
//     op (read_pick). The op derives the period from the published set.
//     A stops write changes no pick (D-157).
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

// readPick is what a pick being published reads of the stops document
// (D-157), over the published set `set` ("null" before the first
// publish): each stop's record on its first date, the pick rewritten
// when that changes it, and the warnings of the stops it visits that
// are not in service over its period. Nothing without a stops document
// or a pick date; the stops document is never written.
func (s *server) readPick(ctx context.Context, pick *canonicalNetwork, set json.RawMessage) ([]json.RawMessage, error) {
	_, stops, found, err := s.keptHead("stops")
	if err != nil || !found {
		return nil, err
	}
	var warnings []json.RawMessage
	err = s.withModule(ctx, "stops", func(host *WasmHost) error {
		var loadedNet struct {
			NetworkHash string `json:"network_hash"`
		}
		if err := host.InvokeBytes(ctx, opLoadNetwork, []byte(pick.Document), &loadedNet); err != nil {
			return fmt.Errorf("the pick's network: %w", err)
		}
		var loadedStops struct {
			StopsHash string `json:"stops_hash"`
		}
		if err := host.InvokeBytes(ctx, opLoadStops, stops, &loadedStops); err != nil {
			return fmt.Errorf("the stops document: %w", err)
		}
		// cap-stops derives the pick's period from the published set.
		var read struct {
			NetworkHash string            `json:"network_hash"`
			Network     json.RawMessage   `json:"network"`
			Warnings    []json.RawMessage `json:"warnings"`
		}
		if err := host.Invoke(ctx, map[string]any{"op": "read_pick", "network_hash": loadedNet.NetworkHash, "stops_hash": loadedStops.StopsHash, "published": set}, &read); err != nil {
			return err
		}
		warnings = read.Warnings
		if read.NetworkHash != pick.DocumentHash {
			pick.Document, pick.DocumentHash, pick.ProducedBy = string(read.Network), read.NetworkHash, "cap-stops@"+host.Hash()
		}
		return nil
	})
	return warnings, err
}

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
