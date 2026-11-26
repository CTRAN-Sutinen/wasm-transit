package main

// Pins that tombstone compaction skips a retired edit record that another
// log's record names, and drops one that nothing names.

import (
	"encoding/json"
	"net/http/httptest"
	"strings"
	"testing"

	"rustwasm/server/store"
)

func TestCompactionSkipsATombstoneThatAnAlertNames(t *testing.T) {
	s := newStoreServer(t, store.NewMemory())
	closeModules(t, s)
	s.kept().Seed(nil)
	mux := storeMux(s)
	mux.HandleFunc("/service-alerts", s.kindRoutes().HandleKindPost)
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	base := srv.URL

	// Both detours are past any horizon (clock 100 ms).
	for _, id := range []string{"named", "free"} {
		if code, _, body := httpPost(t, base+"/edits", editDoc(id, "retired", 3, 100)); code != 200 {
			t.Fatalf("the tombstone %s: %d %s", id, code, body)
		}
	}
	body, _ := json.Marshal(map[string]any{"alerts": map[string]any{"alerts": map[string]any{"a1": map[string]any{
		"status": "published", "effect": "detour", "header": []any{map[string]any{"text": "Route 10 detour"}},
		"informed": []any{map[string]any{"route": "10"}}, "explains": []any{map[string]any{"kind": "edit", "id": "named"}},
	}}}})
	if code, _, resp := httpPost(t, base+"/service-alerts", string(body)); code != 200 {
		t.Fatalf("the alert: %d %s", code, resp)
	}

	s.maybeCompactEdits()

	_, _, doc := httpGet(t, base+"/edits")
	if !strings.Contains(doc, `"id":"named"`) || strings.Contains(doc, `"id":"free"`) {
		t.Fatalf("the named tombstone stays and the free one leaves: %s", doc)
	}
}
