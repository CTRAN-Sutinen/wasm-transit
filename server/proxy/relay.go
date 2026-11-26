package proxy

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"time"

	"rustwasm/server/httpx"
)

const (
	proxyTimeoutDefault = 15 * time.Second
	MaxBytes            = 32 << 20
)

// Relay is the dev relay for the agency's realtime feeds (-rt-proxy). It
// relays any http(s) URL for any caller, so the server binds localhost
// when it is on. Timeout 0 means proxyTimeoutDefault.
type Relay struct {
	Timeout time.Duration
}

func (p *Relay) HandleRelay(w http.ResponseWriter, r *http.Request) {
	raw := r.URL.Query().Get("url")
	if raw == "" {
		http.Error(w, "missing url parameter", http.StatusBadRequest)
		return
	}
	target, err := url.Parse(raw)
	if err != nil || (target.Scheme != "http" && target.Scheme != "https") {
		http.Error(w, "url must be http(s)", http.StatusBadRequest)
		return
	}
	timeout := p.Timeout
	if timeout == 0 {
		timeout = proxyTimeoutDefault
	}
	ctx, cancel := context.WithTimeout(r.Context(), timeout)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, raw, nil)
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	resp, err := (&http.Client{Transport: httpx.Outbound}).Do(req)
	if err != nil {
		http.Error(w, fmt.Sprintf("upstream fetch failed: %v", err), http.StatusBadGateway)
		return
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, MaxBytes+1))
	if err != nil {
		http.Error(w, fmt.Sprintf("upstream read failed: %v", err), http.StatusBadGateway)
		return
	}
	if len(body) > MaxBytes {
		http.Error(w, "upstream answer past the proxy's size cap", http.StatusBadGateway)
		return
	}
	if ct := resp.Header.Get("Content-Type"); ct != "" {
		w.Header().Set("Content-Type", ct)
	}
	w.Header().Set("Access-Control-Allow-Origin", "*")
	w.WriteHeader(resp.StatusCode)
	w.Write(body)
}
