package main

// store verify folds each log's checkpoints through cap-log (D-197): a
// log the module wrote verifies, and a checkpoint whose bytes are not
// the fold is a break.

import (
	"context"
	"errors"
	"strings"
	"testing"

	"rustwasm/server/store"
	"rustwasm/server/wasmhost"
)

const logWasm = "../../../web/pkg/cap_log.wasm"

type rawJSON string

func (r rawJSON) MarshalJSON() ([]byte, error) { return []byte(r), nil }

// writeLog writes three entries to a log through cap-log, each changing
// record A of `kind`, and stores the checkpoint at entry 2.
func writeLog(t *testing.T, host *wasmhost.Host, s *store.Store, log, kind string, values []string) {
	t.Helper()
	ctx := context.Background()
	var checkpoint any
	for seq, value := range values {
		header := map[string]any{"log": log, "seq": seq + 1, "writer": "w", "at_ms": 1, "module": "cap-log@" + host.Hash(), "intent": map[string]any{"op": "test"}}
		req := map[string]any{"op": "compose", "header": header,
			"next": []any{map[string]any{"kind": kind, "version": 1, "records": map[string]any{"A": rawJSON(value)}}}}
		if checkpoint != nil {
			req["checkpoint"] = checkpoint
		}
		var composed struct {
			Entry string `json:"entry"`
		}
		if err := host.Invoke(ctx, req, &composed); err != nil {
			t.Fatal(err)
		}
		if err := s.AppendEntry(log, int64(seq+1), []byte(composed.Entry)); err != nil {
			t.Fatal(err)
		}
		entries, _ := s.Entries(log, 0, int64(seq+1))
		var folded struct {
			Checkpoint string `json:"checkpoint"`
		}
		if err := host.Invoke(ctx, map[string]any{"op": "fold", "log": log, "entries": entries}, &folded); err != nil {
			t.Fatal(err)
		}
		checkpoint = rawJSON(folded.Checkpoint)
		if seq == 1 {
			if err := s.PutCheckpoint(log, 2, []byte(folded.Checkpoint)); err != nil {
				t.Fatal(err)
			}
		}
	}
}

func TestVerifyFoldsEachLogThroughCapLog(t *testing.T) {
	ctx := context.Background()
	host, err := wasmhost.New(ctx, logWasm)
	if err != nil {
		t.Fatalf("cap-log (run `just caps`): %v", err)
	}
	b := store.NewMemory()
	s := store.New(b)
	writeLog(t, host, s, "stops", "stop", []string{`[1]`, `[1,2]`, `[1,2,3]`})
	writeLog(t, host, s, "disruptions", "edit_record", []string{`{"id":"A","rev":1}`, `{"id":"A","rev":2}`, `{"id":"A","rev":3}`})
	fold := folder(ctx, logWasm)
	r, err := store.VerifyFolding(ctx, b, fold)
	if err != nil || r.Logs != 2 || r.Entries != 6 || r.Folded != 2 {
		t.Fatalf("the logs cap-log wrote verify: %+v %v", r, err)
	}

	// A checkpoint at 3 that says seq 3 and holds the fold at 2.
	cp, err := s.Checkpoint("disruptions", 2)
	if err != nil {
		t.Fatal(err)
	}
	bad := strings.Replace(string(cp), `"seq":2`, `"seq":3`, 1)
	if err := b.Create(ctx, "logs/disruptions/checkpoints/00000000000000000003.json", []byte(bad), nil); err != nil {
		t.Fatal(err)
	}
	var ve *store.VerifyError
	if _, err := store.VerifyFolding(ctx, b, fold); !errors.As(err, &ve) || !strings.Contains(ve.Reason, "differs from the fold of entries 3..3") {
		t.Fatalf("a checkpoint that is not the fold: %v", err)
	}
}
