package main

// The geocoder proxy (docs/architecture/web.md, "External services"). The
// page sends only what the user did. The deployment's query policy
// (layers, distance, count) is applied here, so every host that asks
// through this path asks the same question. This host forwards and
// decides nothing.

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strings"
	"time"
)

// Anything else that the page sends is dropped.
var geocodeInputs = map[string][]string{
	"autocomplete": {"text", "focus.point.lat", "focus.point.lon"},
	"search":       {"text", "focus.point.lat", "focus.point.lon"},
	"reverse":      {"point.lat", "point.lon"},
}

var geocodeClient = &http.Client{Transport: outbound, Timeout: 5 * time.Second}

type geocoder struct {
	url    string
	policy map[string]map[string]any
}

// An op that the policy file leaves out is asked with no policy.
func loadGeocoder(base, policyPath string) (*geocoder, error) {
	g := &geocoder{url: base, policy: map[string]map[string]any{}}
	if policyPath == "" {
		return g, nil
	}
	body, err := os.ReadFile(policyPath)
	if err != nil {
		return nil, err
	}
	if err := json.Unmarshal(body, &g.policy); err != nil {
		return nil, fmt.Errorf("%s: %w", policyPath, err)
	}
	for op := range g.policy {
		if _, ok := geocodeInputs[op]; !ok {
			return nil, fmt.Errorf("%s: unknown geocoder op %q", policyPath, op)
		}
	}
	return g, nil
}

func (g *geocoder) service() Service {
	return Service{Path: "/geocode", Query: g.policy}
}

func (s *server) handleGeocode(w http.ResponseWriter, r *http.Request) {
	if s.geocoder == nil {
		http.Error(w, "no geocoder configured (-geocoder)", http.StatusNotFound)
		return
	}
	op := strings.TrimPrefix(r.URL.Path, "/geocode/")
	inputs, ok := geocodeInputs[op]
	if !ok {
		http.Error(w, "unknown geocoder op", http.StatusNotFound)
		return
	}
	target, err := url.JoinPath(s.geocoder.url, "v1", op)
	if err != nil {
		http.Error(w, "bad geocoder url", http.StatusInternalServerError)
		return
	}
	asked := r.URL.Query()
	query := url.Values{}
	for _, k := range inputs {
		if v := asked.Get(k); v != "" {
			query.Set(k, v)
		}
	}
	for k, v := range s.geocoder.policy[op] {
		query.Set(k, fmt.Sprint(v))
	}
	resp, err := geocodeClient.Get(target + "?" + query.Encode())
	if err != nil {
		http.Error(w, "geocoder unreachable", http.StatusBadGateway)
		return
	}
	defer resp.Body.Close()
	if ct := resp.Header.Get("Content-Type"); ct != "" {
		w.Header().Set("Content-Type", ct)
	}
	w.WriteHeader(resp.StatusCode)
	io.Copy(w, resp.Body)
}
