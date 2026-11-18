package main

// A write that carries fields this build does not know (D-167): the
// receive path keeps them and flags them for a person, never refuses
// them. The response names them in X-Unknown-Fields; a write that
// carries none sends no header.

import (
	"encoding/json"
	"net/http/httptest"
	"strings"
	"testing"

	"rustwasm/server/store"
)

func TestAStopsWriteKeepsAndFlagsFieldsThisBuildDoesNotKnow(t *testing.T) {
	_, base := newStopsServer(t, store.NewMemory())
	seed := seededStops(t)
	stops := seed["stops"].(map[string]any)
	first := stops["A"].([]any)[0].(map[string]any)
	first["shelter_model"] = "Tolar 2027"
	seed["survey_round"] = 3
	body, _ := json.Marshal(map[string]any{"from": "", "stops": seed})
	code, hdr, resp := httpPost(t, base+"/stops", string(body))
	if code != 200 {
		t.Fatalf("a field this build does not know is never refused: %d %s", code, resp)
	}
	if got := hdr.Get(unknownFieldsHeader); got != "shelter_model, survey_round" {
		t.Fatalf("the response names the unknown fields: %q", got)
	}
	var out map[string]any
	json.Unmarshal([]byte(resp), &out)
	_, _, blob := httpGet(t, base+"/blobs/"+out["stops_hash"].(string))
	if !strings.Contains(blob, `"shelter_model":"Tolar 2027"`) || !strings.Contains(blob, `"survey_round":3`) {
		t.Fatalf("the stored document keeps them: %s", blob)
	}
}

func TestAnEditsWriteKeepsAndFlagsFieldsThisBuildDoesNotKnow(t *testing.T) {
	srv := httptest.NewServer(storeMux(newStoreServer(t, store.NewMemory())))
	t.Cleanup(srv.Close)
	base := srv.URL
	newer := strings.Replace(editDoc("e1", "draft", 1, 100), `"route_id":"10"`, `"route_id":"10","flagger_needed":true`, 1)
	code, hdr, body := httpPost(t, base+"/edits", newer)
	if code != 200 {
		t.Fatalf("a field this build does not know is never refused: %d %s", code, body)
	}
	if got := hdr.Get(unknownFieldsHeader); got != "flagger_needed" {
		t.Fatalf("the response names the unknown field: %q", got)
	}
	if !strings.Contains(body, `"flagger_needed":true`) {
		t.Fatalf("the merged document keeps it: %s", body)
	}
	code, hdr, _ = httpPost(t, base+"/edits", editDoc("e2", "draft", 1, 200))
	if code != 200 || hdr.Get(unknownFieldsHeader) != "" {
		t.Fatalf("a write with every field known sends no header: %d %q", code, hdr.Get(unknownFieldsHeader))
	}
}
