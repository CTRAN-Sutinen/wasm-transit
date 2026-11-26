package imports

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
// writes nothing without such a kind (e, from the table's feed_events),
// without its log, or when nothing differs.
func RecordFeed(ctx context.Context, ls *logs.Logs, e *kinds.Entry, feed *Feed) error {
	if e == nil {
		return nil
	}
	l := ls.Of(e.StoreKind)
	if l == nil {
		return nil
	}
	before := ls.Seq(e.StoreKind)
	op, _ := json.Marshal(map[string]any{"cap": e.Module, "op": e.FeedEvents, "args": map[string]any{"network_hash": feed.NetworkHash, "feed": feed.Hash}})
	w := logs.Write{Writer: "import:feed", Op: e.FeedEvents, ArgsHash: feed.Hash}
	seq, _, _, refusal, err := ls.Receive(ctx, nil, l, w, logs.Call{Ops: []json.RawMessage{op}, Loads: []logs.Load{{Code: wasmhost.OpLoadNetwork, Doc: feed.Document}}})
	if err != nil {
		return err
	}
	if refusal != nil {
		return fmt.Errorf("the recorded document is refused: %v", refusal.Body)
	}
	if seq > before {
		log.Printf("%s: the feed %s recorded its changes in entry %d", e.StoreKind, feed.Hash, seq)
	}
	return nil
}
