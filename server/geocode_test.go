package main

// The geocoder proxy forwards the user's input and the deployment's
// query policy — never a policy the page sends — refuses ops it does
// not carry, answers 404 with no geocoder configured, and the
// manifest lists the service exactly when it is configured.

import (
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
)

func TestGeocodeProxy(t *testing.T) {
	var asked map[string][]string
	pelias := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		asked = map[string][]string(r.URL.Query())
		asked["path"] = []string{r.URL.Path}
		w.Header().Set("Content-Type", "application/json")
		io.WriteString(w, `{"type":"FeatureCollection","features":[]}`)
	}))
	defer pelias.Close()

	policy := filepath.Join(t.TempDir(), "geocoder.json")
	os.WriteFile(policy, []byte(`{"reverse":{"size":1,"layers":"address,street","boundary.circle.radius":0.25},"autocomplete":{"size":5}}`), 0o644)
	g, err := loadGeocoder(pelias.URL, policy)
	if err != nil {
		t.Fatal(err)
	}
	s := &server{wasm: newTestHost(t), geocoder: g}
	mux := http.NewServeMux()
	mux.HandleFunc("/geocode/", s.handleGeocode)
	mux.HandleFunc("/manifest", s.handleManifest)
	srv := httptest.NewServer(mux)
	defer srv.Close()

	get := func(path string) (int, string) {
		t.Helper()
		resp, err := http.Get(srv.URL + path)
		if err != nil {
			t.Fatal(err)
		}
		defer resp.Body.Close()
		body, _ := io.ReadAll(resp.Body)
		return resp.StatusCode, string(body)
	}
	one := func(k string) string {
		if v := asked[k]; len(v) == 1 {
			return v[0]
		}
		return ""
	}

	// The user's input crosses; the page's own policy does not — the
	// deployment's does.
	code, body := get("/geocode/reverse?point.lat=45.6&point.lon=-122.5&layers=venue&size=40&api_key=x")
	if code != 200 || body != `{"type":"FeatureCollection","features":[]}` {
		t.Fatalf("reverse: %d %s", code, body)
	}
	want := map[string]string{
		"path": "/v1/reverse", "point.lat": "45.6", "point.lon": "-122.5",
		"layers": "address,street", "size": "1", "boundary.circle.radius": "0.25", "api_key": "",
	}
	for k, v := range want {
		if one(k) != v {
			t.Fatalf("reverse forwarded %s=%q, want %q (all: %v)", k, one(k), v, asked)
		}
	}

	get("/geocode/autocomplete?text=1400%20NE%20136th&focus.point.lat=45.6&focus.point.lon=-122.5")
	if one("path") != "/v1/autocomplete" || one("text") != "1400 NE 136th" || one("focus.point.lat") != "45.6" || one("size") != "5" {
		t.Fatalf("autocomplete forwarded %v", asked)
	}

	asked = nil
	if code, _ := get("/geocode/place?ids=x"); code != 404 || asked != nil {
		t.Fatalf("unknown op: %d, forwarded %v", code, asked)
	}

	// The manifest lists the service and the policy its path applies.
	var m Manifest
	_, body = get("/manifest")
	if err := json.Unmarshal([]byte(body), &m); err != nil {
		t.Fatal(err)
	}
	if m.Services["geocoder"].Path != "/geocode" || m.Services["geocoder"].Query["reverse"]["layers"] != "address,street" {
		t.Fatalf("manifest services: %+v", m.Services)
	}

	// No geocoder: 404, and the manifest lists no service.
	s.geocoder = nil
	if code, _ := get("/geocode/reverse?point.lat=45.6&point.lon=-122.5"); code != 404 {
		t.Fatalf("unconfigured: %d", code)
	}
	_, body = get("/manifest")
	m = Manifest{}
	json.Unmarshal([]byte(body), &m)
	if m.Services != nil {
		t.Fatalf("unconfigured manifest lists %+v", m.Services)
	}

	// A policy for an op the proxy does not carry is refused at start.
	os.WriteFile(policy, []byte(`{"place":{"size":1}}`), 0o644)
	if _, err := loadGeocoder(pelias.URL, policy); err == nil {
		t.Fatal("a policy for an unknown op loaded")
	}
}
