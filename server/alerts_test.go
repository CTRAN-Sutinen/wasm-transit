package main

// The service alerts and alert templates documents in store mode
// (D-206, D-210): each write goes through the receive path (cap-alerts
// validates it and answers its canonical bytes), a stale write is
// refused, the manifest pins each head, /blobs serves each by hash, and
// both land in one log, the alerts log.

import (
	"encoding/json"
	"net/http/httptest"
	"testing"

	"rustwasm/server/store"
)

func TestAlertsWritesAreValidatedAndLogged(t *testing.T) {
	s := newStoreServer(t, store.NewMemory())
	closeModules(t, s)
	mux := storeMux(s)
	mux.HandleFunc("/service-alerts", s.handleKeptRoute)
	mux.HandleFunc("/alert-templates", s.handleKeptRoute)
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	base := srv.URL

	post := func(path, field, from string, doc any) (int, map[string]any) {
		body, _ := json.Marshal(map[string]any{"from": from, field: doc})
		code, _, resp := httpPost(t, base+path, string(body))
		var out map[string]any
		json.Unmarshal([]byte(resp), &out)
		return code, out
	}
	alert := func(header string) map[string]any {
		return map[string]any{"alerts": map[string]any{"a1": map[string]any{
			"status": "published", "effect": "detour", "header": []any{map[string]any{"text": header}},
			"informed": []any{map[string]any{"route": "10"}},
		}}}
	}
	if code, out := post("/service-alerts", "alerts", "", alert(" ")); code != 422 {
		t.Fatalf("an alert with no header is refused: %d %v", code, out)
	}
	code, out := post("/service-alerts", "alerts", "", alert("Route 10 detour"))
	if code != 200 {
		t.Fatalf("an alert: %d %v", code, out)
	}
	alertsHash := out["alerts_hash"].(string)
	if code, out := post("/service-alerts", "alerts", "", alert("Route 10 detour")); code != 409 {
		t.Fatalf("a write from a version that is no longer the head is refused: %d %v", code, out)
	}
	templates := map[string]any{"templates": map[string]any{"closure": map[string]any{
		"name": "Stop closure", "header": []any{map[string]any{"text": "The stop at {stop} is closed"}},
	}}}
	code, out = post("/alert-templates", "templates", "", templates)
	if code != 200 {
		t.Fatalf("a template: %d %v", code, out)
	}
	templatesHash := out["templates_hash"].(string)

	_, _, body := httpGet(t, base+"/manifest")
	var m struct {
		ServiceAlertsHash  string `json:"service_alerts_hash"`
		AlertTemplatesHash string `json:"alert_templates_hash"`
	}
	json.Unmarshal([]byte(body), &m)
	if m.ServiceAlertsHash != alertsHash || m.AlertTemplatesHash != templatesHash {
		t.Fatalf("the manifest pins both heads: %+v vs %s %s", m, alertsHash, templatesHash)
	}
	for hash, kind := range map[string]string{alertsHash: "service_alerts", templatesHash: "alert_templates"} {
		code, hdr, blob := httpGet(t, base+"/blobs/"+hash)
		if code != 200 || hdr.Get("X-Document-Kind") != kind || sha([]byte(blob)) != hash {
			t.Fatalf("the %s blob: %d kind=%q", kind, code, hdr.Get("X-Document-Kind"))
		}
	}
	// A writer one version behind posts v1; it lands as v2 (D-216).
	v1 := alert("Route 10 detour, still")
	v1["kind"], v1["schema_version"] = "service_alerts", 1
	code, out = post("/service-alerts", "alerts", alertsHash, v1)
	if code != 200 {
		t.Fatalf("a v1 document comes forward at receive: %d %v", code, out)
	}
	_, v2hdr, v2 := httpGet(t, base+"/blobs/"+out["alerts_hash"].(string))
	var stored struct {
		SchemaVersion int `json:"schema_version"`
	}
	json.Unmarshal([]byte(v2), &stored)
	if stored.SchemaVersion != 2 || v2hdr.Get("X-Schema-Version") != "2" {
		t.Fatalf("the stored document is v2, and its blob says so: %q %s", v2hdr.Get("X-Schema-Version"), v2)
	}

	code, _, entries := httpGet(t, base+"/logs/alerts")
	var log struct {
		Entries []json.RawMessage `json:"entries"`
	}
	json.Unmarshal([]byte(entries), &log)
	if code != 200 || len(log.Entries) != 3 {
		t.Fatalf("every write is an entry of the alerts log: %d %s", code, entries)
	}
}
