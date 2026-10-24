package main

// The golden corpus replay: the same request→wrapper-text pairs the
// native Rust tests and the Node smoke run. Byte-identical outputs
// from the same artifact bytes — the cross-runtime equivalence proof.
//
// The host never re-marshals the responses: the comparison is against
// the artifact's raw output, so Go's JSON quirks cannot leak in.

import (
	"context"
	"encoding/json"
	"os"
	"strconv"
	"strings"
	"testing"
)

func TestGoldenCorpusReplay(t *testing.T) {
	root := ".."
	body, err := os.ReadFile(root + "/testdata/corpus.json")
	if err != nil {
		t.Fatalf("read corpus: %v (run `just corpus`)", err)
	}
	var corpus struct {
		Fixture string `json:"fixture"`
		Steps   []struct {
			Name    string  `json:"name"`
			Request string  `json:"request"`
			BytesOp *uint32 `json:"bytes_op"`
			Bind    string  `json:"bind"`
			Expect  string  `json:"expect"`
		} `json:"steps"`
	}
	if err := json.Unmarshal(body, &corpus); err != nil {
		t.Fatalf("parse corpus: %v", err)
	}
	fixture, err := os.ReadFile(root + "/" + corpus.Fixture)
	if err != nil {
		t.Fatalf("read fixture: %v", err)
	}

	host := newTestHost(t)
	ctx := context.Background()
	feedID := ""
	netFeedID := ""

	// The registry assigns feed ids; this captures one from a load
	// outcome so the corpus replays regardless of how many feeds
	// earlier tests registered.
	bindID := func(got []byte) string {
		var parsed struct {
			OK struct {
				FeedID uint32 `json:"feed_id"`
			} `json:"ok"`
		}
		if err := json.Unmarshal(got, &parsed); err != nil {
			t.Fatalf("parse load outcome: %v", err)
		}
		return strconv.FormatUint(uint64(parsed.OK.FeedID), 10)
	}

	for _, step := range corpus.Steps {
		var got []byte
		if step.BytesOp != nil {
			var err error
			got, err = host.call(ctx, host.invokeBytes, fixture, uint64(*step.BytesOp))
			if err != nil {
				t.Fatalf("%s: invoke: %v", step.Name, err)
			}
			feedID = bindID(got)
		} else {
			if strings.Contains(step.Request, "$FEED") && feedID == "" {
				t.Fatalf("%s: uses $FEED before the load step", step.Name)
			}
			if strings.Contains(step.Request, "$NET") && netFeedID == "" {
				t.Fatalf("%s: uses $NET before the network_load step", step.Name)
			}
			request := strings.ReplaceAll(strings.ReplaceAll(step.Request, "$FEED", feedID), "$NET", netFeedID)
			var err error
			got, err = host.call(ctx, host.invoke, []byte(request))
			if err != nil {
				t.Fatalf("%s: invoke: %v", step.Name, err)
			}
		}
		// A step with bind registers a second feed (the network
		// document); its id is captured like the bytes load above.
		if step.Bind == "$NET" {
			netFeedID = bindID(got)
		}
		want := strings.ReplaceAll(strings.ReplaceAll(step.Expect, "$FEED", feedID), "$NET", netFeedID)
		if string(got) != want {
			t.Fatalf("corpus step %s diverges:\n got  %s\n want %s", step.Name, got, want)
		}
	}
}
