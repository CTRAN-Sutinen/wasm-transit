package main

// Compaction and references (D-231): the tombstone compaction skips a
// retired edit record that a held record of another log names, so
// compaction alone never breaks a reference. A tombstone that nothing
// names still leaves.

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
	s.seedKept(nil)
	mux := storeMux(s)
	mux.HandleFunc("/service-alerts", s.handleKeptRoute)
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	base := srv.URL

	// Two detours retired long ago (clock 100 ms), past any horizon.
	for _, id := range []string{"named", "free"} {
		if code, _, body := httpPost(t, base+"/edits", editDoc(id, "retired", 3, 100)); code != 200 {
			t.Fatalf("the tombstone %s: %d %s", id, code, body)
		}
	}
	// An alert explains the first one.
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
