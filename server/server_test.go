package main

// The round-trip proof: the server loads the exact artifact bytes the
// browser runs (sha256 pinned in the manifest) and gets the same
// domain answers — feeds, boards, plans, edit documents. This is the
// "same artifact everywhere" claim, executed, not asserted.

import (
	"bytes"
	"context"
	"io"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"testing"
	"time"
)

const date = "2026-10-23" // Friday: WK + HOL both run

// fixtureZip mirrors the e2e fixture (web/e2e/make_fixtures.py).
func fixtureZip(t *testing.T) []byte {
	t.Helper()
	files := map[string]string{
		"agency.txt": "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n",
		"routes.txt": "route_id,agency_id,route_short_name,route_type\n10,1,10,3\n20,1,20,3\n",
		"stops.txt": "stop_id,stop_name,stop_lat,stop_lon\n" +
			"A,Main & 1st,45.500,-73.500\nB,Main & 2nd,45.510,-73.510\nC,Main & 3rd,45.520,-73.520\n",
		"trips.txt": "route_id,service_id,trip_id,direction_id,block_id,shape_id\n" +
			"10,WK,t10,0,B1,sh1\n20,WK,t20,0,B1,\n10,WK,t30,0,,\n10,SUN,t11,0,,\n20,HOL,t12,0,B2,\n",
		"stop_times.txt": "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n" +
			"t10,08:00:00,08:00:00,A,1\nt10,08:10:00,08:10:00,B,2\n" +
			"t20,08:20:00,08:20:00,A,1\nt20,08:30:00,08:30:00,B,2\n" +
			"t30,08:40:00,08:40:00,A,1\nt30,08:45:00,08:45:00,C,2\nt30,08:50:00,08:50:00,B,3\n" +
			"t11,09:00:00,09:00:00,A,1\nt11,09:10:00,09:10:00,B,2\n" +
			"t12,07:30:00,07:30:00,B,1\n",
		"calendar.txt": "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\n" +
			"WK,1,1,1,1,1,0,0,20260101,20261231\nSUN,0,0,0,0,0,0,1,20260101,20261231\n",
		"calendar_dates.txt": "service_id,date,exception_type\nHOL,20261023,1\n",
		"shapes.txt": "shape_id,shape_pt_lat,shape_pt_lon,shape_pt_sequence\n" +
			"sh1,45.400,-73.400,1\nsh1,45.500,-73.500,2\nsh1,45.510,-73.510,3\n",
		"feed_info.txt": "feed_publisher_name,feed_publisher_url,feed_lang,feed_version\n" +
			"Test Transit,https://example.com,en,2026.10.1\n",
	}
	var buf bytes.Buffer
	if err := writeZip(&buf, files); err != nil {
		t.Fatalf("fixture zip: %v", err)
	}
	return buf.Bytes()
}

func newTestHost(t *testing.T) *WasmHost {
	t.Helper()
	host, err := NewWasmHost(context.Background(), "../web/pkg/transit_wasm.wasm")
	if err != nil {
		t.Fatalf("load artifact: %v", err)
	}
	return host
}

func boardAt(t *testing.T, host *WasmHost, feedID uint32, stop string) []string {
	t.Helper()
	var resp struct {
		Departures []struct {
			TripID string `json:"trip_id"`
			Time   string `json:"departure_time_hhmmss"`
		} `json:"departures"`
	}
	if err := host.Invoke(context.Background(), map[string]any{
		"op": "departures", "feed_id": feedID, "stop_id": stop,
		"date": date, "from_secs": 0, "limit": 10,
	}, &resp); err != nil {
		t.Fatalf("departures: %v", err)
	}
	var out []string
	for _, d := range resp.Departures {
		out = append(out, d.TripID+"@"+d.Time)
	}
	return out
}

func wantBoard(t *testing.T, got []string, want ...string) {
	t.Helper()
	if strings.Join(got, ",") != strings.Join(want, ",") {
		t.Fatalf("board mismatch:\n got  %v\n want %v", got, want)
	}
}

func TestArtifactRoundTrip(t *testing.T) {
	ctx := context.Background()
	host := newTestHost(t)

	if !strings.HasPrefix(host.Hash(), "sha256:") {
		t.Fatalf("host hash not computed: %q", host.Hash())
	}

	feedID, err := host.LoadFeed(ctx, fixtureZip(t))
	if err != nil {
		t.Fatalf("load_feed: %v", err)
	}

	// The manifest's version field: parsed by the artifact, not Go.
	info, err := host.FeedInfo(ctx, feedID)
	if err != nil {
		t.Fatalf("feed_info: %v", err)
	}
	if info == nil || info.FeedVersion == nil || *info.FeedVersion != "2026.10.1" {
		t.Fatalf("feed_info version: %+v", info)
	}

	// Baseline board matches the fixture's known schedule.
	wantBoard(t, boardAt(t, host, feedID, "B"),
		"t12@07:30:00", "t10@08:10:00", "t20@08:30:00", "t30@08:50:00")

	// The full edit-document flow: push, effective board, compare.
	record := map[string]any{
		"id": "e1",
		"diversion": map[string]any{
			"id": "d1", "route_id": "10", "direction_id": nil,
			"first_kept_stop": "A", "last_kept_stop": "B",
			"skipped_stops": []string{"C"}, "detour": nil,
			"added_secs": 300, "start_date": date, "end_date": date,
		},
		"status": "draft", "rev": 1, "updated_at": 1,
	}
	var reports struct {
		Reports []struct {
			Valid         bool `json:"valid"`
			AffectedTrips int  `json:"affected_trips"`
		} `json:"reports"`
	}
	if err := host.Invoke(ctx, map[string]any{
		"op": "set_diversions", "feed_id": feedID, "records": []any{record},
	}, &reports); err != nil {
		t.Fatalf("set_diversions: %v", err)
	}
	if len(reports.Reports) != 1 || !reports.Reports[0].Valid || reports.Reports[0].AffectedTrips != 3 {
		t.Fatalf("reports: %+v", reports)
	}

	wantBoard(t, boardAt(t, host, feedID, "B"),
		"t12@07:30:00", "t10@08:15:00", "t20@08:30:00", "t30@08:55:00")
	if got := boardAt(t, host, feedID, "C"); len(got) != 0 {
		t.Fatalf("C should lose service, got %v", got)
	}

	var cmp struct {
		Active struct {
			Arrival string `json:"arrival_hhmmss"`
		} `json:"active"`
		Baseline struct {
			Arrival string `json:"arrival_hhmmss"`
		} `json:"baseline"`
	}
	if err := host.Invoke(ctx, map[string]any{
		"op": "plan_trip_compare", "feed_id": feedID,
		"from": "A", "to": "B", "date": date, "depart_secs": 27000, "max_legs": 6,
	}, &cmp); err != nil {
		t.Fatalf("plan_trip_compare: %v", err)
	}
	if cmp.Active.Arrival != "08:15:00" || cmp.Baseline.Arrival != "08:10:00" {
		t.Fatalf("compare: active %s baseline %s", cmp.Active.Arrival, cmp.Baseline.Arrival)
	}

	// Retire = tombstone; DropFeed releases the registry entry.
	retired := map[string]any{
		"id": "e1", "diversion": record["diversion"],
		"status": "retired", "rev": 2, "updated_at": 2,
	}
	if err := host.Invoke(ctx, map[string]any{
		"op": "set_diversions", "feed_id": feedID, "records": []any{retired},
	}, &struct{}{}); err != nil {
		t.Fatalf("set_diversions (retire): %v", err)
	}
	wantBoard(t, boardAt(t, host, feedID, "B"),
		"t12@07:30:00", "t10@08:10:00", "t20@08:30:00", "t30@08:50:00")

	if err := host.DropFeed(ctx, feedID); err != nil {
		t.Fatalf("drop_feed: %v", err)
	}
	err = host.Invoke(ctx, map[string]any{"op": "summary", "feed_id": feedID}, &struct{}{})
	if err == nil || !strings.Contains(err.Error(), "unknown feed_id") {
		t.Fatalf("dropped feed must error, got %v", err)
	}
}

// TestArtifactIsTheDeployedBytes pins the artifact the tests run to
// the file the browser fetches — same bytes, by construction of this
// test reading that exact path.
func TestArtifactIsTheDeployedBytes(t *testing.T) {
	host := newTestHost(t)
	sum := sha256File(t, "../web/pkg/transit_wasm.wasm")
	if host.Hash() != sum {
		t.Fatalf("host hash %s != file hash %s", host.Hash(), sum)
	}
}

// TestEditsSyncEndpoint exercises POST/GET /edits through real HTTP:
// the client document merges with the server's (LWW in the artifact),
// drafts submit, tombstones propagate, and persistence round-trips.
func TestEditsSyncEndpoint(t *testing.T) {
	host := newTestHost(t)

	s := &server{wasm: host, editsPath: t.TempDir() + "/edits.json"}
	mux := http.NewServeMux()
	mux.HandleFunc("/edits", s.handleEdits)
	base := httptest.NewServer(mux).URL

	editJSON := func(id, status string, rev, at int64) string {
		diversion := `{"id":"d-` + id + `","route_id":"10","direction_id":null,` +
			`"first_kept_stop":"A","last_kept_stop":"B","skipped_stops":["C"],` +
			`"detour":null,"added_secs":300,"start_date":"` + date + `","end_date":"` + date + `"}`
		return `[{"id":"` + id + `","diversion":` + diversion +
			`,"status":"` + status + `","rev":` + strconv.FormatInt(rev, 10) +
			`,"updated_at":` + strconv.FormatInt(at, 10) + `}]`
	}
	post := func(doc string) (int, string) {
		resp, err := http.Post(base+"/edits", "application/json", strings.NewReader(doc))
		if err != nil {
			t.Fatalf("post: %v", err)
		}
		defer resp.Body.Close()
		body, _ := io.ReadAll(resp.Body)
		return resp.StatusCode, string(body)
	}

	// Push a draft; the server must submit it.
	code, body := post(editJSON("e1", "draft", 1, 100))
	if code != 200 {
		t.Fatalf("post: %d %s", code, body)
	}
	if !strings.Contains(body, `"status":"submitted"`) {
		t.Fatalf("draft not submitted: %s", body)
	}

	// A second push with a newer tombstone for the same id: LWW. The
	// timestamp must beat the server's submit stamp (real epoch ms),
	// so date it in the future.
	code, body = post(editJSON("e1", "retired", 2, time.Now().UnixMilli()+60_000))
	if code != 200 {
		t.Fatalf("post 2: %d %s", code, body)
	}
	if !strings.Contains(body, `"status":"retired"`) {
		t.Fatalf("tombstone did not win: %s", body)
	}

	// GET returns the merged document.
	resp, err := http.Get(base + "/edits")
	if err != nil {
		t.Fatalf("get: %v", err)
	}
	got, _ := io.ReadAll(resp.Body)
	resp.Body.Close()
	if !strings.Contains(string(got), `"id":"e1"`) || !strings.Contains(string(got), `"retired"`) {
		t.Fatalf("get: %s", got)
	}

	// Persistence: a fresh server with the same path restores the doc.
	s2 := &server{wasm: host, edits: loadEdits(s.editsPath), editsPath: s.editsPath}
	mux2 := http.NewServeMux()
	mux2.HandleFunc("/edits", s2.handleEdits)
	base2 := httptest.NewServer(mux2).URL
	resp2, err := http.Get(base2 + "/edits")
	if err != nil {
		t.Fatalf("get 2: %v", err)
	}
	body2, _ := io.ReadAll(resp2.Body)
	resp2.Body.Close()
	if !strings.Contains(string(body2), `"retired"`) {
		t.Fatalf("persisted document lost the tombstone: %s", body2)
	}
}
