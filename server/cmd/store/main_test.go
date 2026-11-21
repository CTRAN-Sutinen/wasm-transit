package main

// store verify folds each log's checkpoints through cap-log (D-197): a
// log the module wrote verifies, and a checkpoint whose bytes are not
// the fold is a break.

import (
	"context"
	"errors"
	"os"
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

// appendEntry composes one entry of the stops log through cap-log over
// the fold of the entries before it, and stores it.
func appendEntry(t *testing.T, host *wasmhost.Host, s *store.Store, seq int, next []any) {
	t.Helper()
	ctx := context.Background()
	header := map[string]any{"log": "stops", "seq": seq, "writer": "w", "at_ms": seq, "module": "cap-log@" + host.Hash(), "intent": map[string]any{"op": "test"}}
	req := map[string]any{"op": "compose", "header": header, "next": next}
	if seq > 1 {
		entries, _ := s.Entries("stops", 0, int64(seq-1))
		var folded struct {
			Checkpoint string `json:"checkpoint"`
		}
		if err := host.Invoke(ctx, map[string]any{"op": "fold", "log": "stops", "entries": entries}, &folded); err != nil {
			t.Fatal(err)
		}
		req["checkpoint"] = rawJSON(folded.Checkpoint)
	}
	var composed struct {
		Entry string `json:"entry"`
	}
	if err := host.Invoke(ctx, req, &composed); err != nil || composed.Entry == "" {
		t.Fatalf("entry %d: %v", seq, err)
	}
	if err := s.AppendEntry("stops", int64(seq), []byte(composed.Entry)); err != nil {
		t.Fatal(err)
	}
}

// The existence check (D-233): verify reports a stop request whose stop
// left the stops log, with the entry that took it out, the kind table
// and the module it judged with, and the log's seq; the store still
// verifies.
func TestVerifyReportsABrokenReferenceWithTheEntryThatBrokeIt(t *testing.T) {
	ctx := context.Background()
	host, err := wasmhost.New(ctx, logWasm)
	if err != nil {
		t.Fatalf("cap-log (run `just caps`): %v", err)
	}
	b := store.NewMemory()
	s := store.New(b)
	stop := func(ids ...string) map[string]any {
		records := map[string]any{}
		for _, id := range ids {
			records[id] = rawJSON(`[{"on":"2020-01-01","event":"in_service","lat":45.5,"lon":-122.6}]`)
		}
		return map[string]any{"kind": "stop", "version": 1, "records": records}
	}
	request := map[string]any{"kind": "stop_request", "version": 1, "records": map[string]any{"R-1": rawJSON(`{"stop":"B","job":"repair_improve","status":"submitted"}`)}}
	appendEntry(t, host, s, 1, []any{stop("A", "B")})
	appendEntry(t, host, s, 2, []any{request})
	appendEntry(t, host, s, 3, []any{stop("A")})

	m := &logModule{ctx: ctx, path: logWasm}
	table, err := os.ReadFile("../../../web/pkg/kinds.json")
	if err != nil {
		t.Fatal(err)
	}
	r, err := store.VerifyChecking(ctx, b, m.fold, m.references(table))
	if err != nil || r.References == nil {
		t.Fatalf("a store with a broken reference still verifies: %+v %v", r, err)
	}
	f := r.References
	if f.KindTable != store.HashOf(table) || !strings.HasPrefix(f.Module, "cap-log@sha256:") || f.Seqs["stops"] != 3 {
		t.Fatalf("the finding names the table, the module, and the seq: %+v", f)
	}
	if len(f.Broken) != 1 || string(f.Broken[0]) != `{"log":"stops","kind":"stop_request","id":"R-1","field":"/stop","target":"stop","names":"B","entry":{"log":"stops","seq":3}}` {
		t.Fatalf("R-1 names B, which entry 3 took out: %s", f.Broken)
	}
}

// store revert lists the records that name what it drops (D-233): a
// frozen version naming the network commit the revert drops is listed,
// with its field and the hash.
func TestRevertListsTheReferencesIntoTheCommitsItDrops(t *testing.T) {
	ctx := context.Background()
	host, err := wasmhost.New(ctx, logWasm)
	if err != nil {
		t.Fatalf("cap-log (run `just caps`): %v", err)
	}
	b := store.NewMemory()
	s := store.New(b)
	env := store.Envelope{Kind: "network_document", SchemaVersion: 2, ProducedBy: "test"}
	first, second := []byte(`{"v":1}`), []byte(`{"v":2}`)
	if _, _, err := s.CAS("plan.fall", "", first, env, store.HashOf(first), "w", 1); err != nil {
		t.Fatal(err)
	}
	if _, _, err := s.CAS("plan.fall", store.HashOf(first), second, env, store.HashOf(second), "w", 2); err != nil {
		t.Fatal(err)
	}
	header := map[string]any{"log": "plans", "seq": 1, "writer": "w", "at_ms": 3, "module": "cap-log@" + host.Hash(), "intent": map[string]any{"op": "test"}}
	frozen := map[string]any{"kind": "frozen_version", "version": 1, "records": map[string]any{"fall@2": map[string]any{"network_hash": store.HashOf(second), "purpose": "p"}}}
	var composed struct {
		Entry string `json:"entry"`
	}
	if err := host.Invoke(ctx, map[string]any{"op": "compose", "header": header, "next": []any{frozen}}, &composed); err != nil || composed.Entry == "" {
		t.Fatalf("the plans log's entry: %v", err)
	}
	if err := s.AppendEntry("plans", 1, []byte(composed.Entry)); err != nil {
		t.Fatal(err)
	}

	r, err := store.Revert(s, "plan.fall", store.HashOf(first), "operator", 4)
	if err != nil || len(r.Dropped) != 1 {
		t.Fatalf("the revert drops the second commit: %+v %v", r, err)
	}
	table, err := os.ReadFile("../../../web/pkg/kinds.json")
	if err != nil {
		t.Fatal(err)
	}
	m := &logModule{ctx: ctx, path: logWasm}
	refs, err := m.naming(b, table, r.Dropped)
	if err != nil {
		t.Fatal(err)
	}
	want := `{"log":"plans","kind":"frozen_version","id":"fall@2","field":"/network_hash","target":"network","names":"` + store.HashOf(second) + `"}`
	if len(refs) != 1 || string(refs[0]) != want {
		t.Fatalf("the frozen version that names the dropped network: %s", refs)
	}
}
