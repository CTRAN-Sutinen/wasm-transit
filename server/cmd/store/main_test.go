package main

// store verify folds the disruptions log's checkpoints through cap-sync:
// a log the module wrote verifies, and a checkpoint whose bytes are not
// the fold is a break.

import (
	"context"
	"errors"
	"strings"
	"testing"

	"rustwasm/server/store"
	"rustwasm/server/wasmhost"
)

const syncWasm = "../../../web/pkg/cap_sync.wasm"

func TestVerifyFoldsTheDisruptionsLogThroughCapSync(t *testing.T) {
	ctx := context.Background()
	host, err := wasmhost.New(ctx, syncWasm)
	if err != nil {
		t.Fatalf("cap-sync (run `just caps`): %v", err)
	}
	b := store.NewMemory()
	s := store.New(b)
	head := `{"kind":"edit_records","schema_version":2,"records":[]}`
	for seq, id := range []string{"e1", "e2", "e3"} {
		next := `{"kind":"edit_records","schema_version":2,"records":[` + record(id) + `]}`
		if seq > 0 {
			next = strings.Replace(head, `"records":[`, `"records":[`+record(id)+`,`, 1)
		}
		var ans struct {
			Entry string `json:"entry"`
		}
		header := map[string]any{"seq": seq + 1, "writer": "w", "at_ms": 1, "module": "cap-sync@" + host.Hash(), "intent": map[string]any{"op": "test"}}
		if err := host.Invoke(ctx, map[string]any{"op": "entry_disruptions", "head": rawJSON(head), "next": rawJSON(next), "header": header}, &ans); err != nil {
			t.Fatal(err)
		}
		if err := s.AppendEntry("disruptions", int64(seq+1), []byte(ans.Entry)); err != nil {
			t.Fatal(err)
		}
		head = next
	}
	fold := folder(ctx, syncWasm)
	entries, _ := s.Entries("disruptions", 0, 2)
	cp, err := fold("disruptions", nil, entries)
	if err != nil {
		t.Fatal(err)
	}
	if err := s.PutCheckpoint("disruptions", 2, cp); err != nil {
		t.Fatal(err)
	}
	r, err := store.VerifyFolding(ctx, b, fold)
	if err != nil || r.Folded != 1 || r.Entries != 3 {
		t.Fatalf("a log cap-sync wrote verifies: %+v %v", r, err)
	}

	// A checkpoint at 3 that says seq 3 and holds the wrong document.
	bad := strings.Replace(string(cp), `"seq":2`, `"seq":3`, 1)
	if err := b.Create(ctx, "logs/disruptions/checkpoints/00000000000000000003.json", []byte(bad), nil); err != nil {
		t.Fatal(err)
	}
	var ve *store.VerifyError
	if _, err := store.VerifyFolding(ctx, b, fold); !errors.As(err, &ve) || !strings.Contains(ve.Reason, "differs from the fold of entries 3..3") {
		t.Fatalf("a checkpoint that is not the fold: %v", err)
	}
}

type rawJSON string

func (r rawJSON) MarshalJSON() ([]byte, error) { return []byte(r), nil }

func record(id string) string {
	return `{"id":"` + id + `","diversion":{"id":"d-` + id + `","route_id":"10","direction_id":null,"first_kept_stop":"A","last_kept_stop":"B","skipped_stops":[],"detour":null,"added_secs":60,"start_date":"2026-10-23","end_date":"2026-10-23"},"status":"accepted","rev":1,"updated_at":1}`
}
