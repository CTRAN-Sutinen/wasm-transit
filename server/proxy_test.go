package main

// The /rt feed proxy is a dev convenience (D-67): a deployment's pages
// fetch the agency's feeds directly (HTTPS with CORS), so the proxy is
// off unless -rt-proxy asks for it — a server beyond localhost never
// becomes an open relay by default. When on, it relays an http(s) URL
// with a timeout and a size cap.

import (
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"
	"time"
)

func TestTheFeedProxyIsOffUnlessAsked(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		t.Errorf("the proxy is off: nothing reaches the upstream")
	}))
	defer upstream.Close()
	srv := httptest.NewServer((&server{dist: t.TempDir()}).routes())
	defer srv.Close()
	resp, err := http.Get(srv.URL + "/rt?url=" + url.QueryEscape(upstream.URL))
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusNotFound {
		t.Fatalf("/rt without -rt-proxy is not a route: %d", resp.StatusCode)
	}
}

func TestTheFeedProxyRelaysWhenAsked(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/x-protobuf")
		w.Write([]byte("feed bytes"))
	}))
	defer upstream.Close()
	srv := httptest.NewServer((&server{dist: t.TempDir(), rtProxy: true}).routes())
	defer srv.Close()
	resp, err := http.Get(srv.URL + "/rt?url=" + url.QueryEscape(upstream.URL))
	if err != nil {
		t.Fatal(err)
	}
	body, _ := io.ReadAll(resp.Body)
	resp.Body.Close()
	if resp.StatusCode != 200 || string(body) != "feed bytes" || resp.Header.Get("Content-Type") != "application/x-protobuf" {
		t.Fatalf("relayed: %d %q %q", resp.StatusCode, body, resp.Header.Get("Content-Type"))
	}
	// Not http(s): refused.
	resp, _ = http.Get(srv.URL + "/rt?url=" + url.QueryEscape("file:///etc/passwd"))
	resp.Body.Close()
	if resp.StatusCode != http.StatusBadRequest {
		t.Fatalf("a non-http URL is refused: %d", resp.StatusCode)
	}
}

func TestTheFeedProxyCapsTimeAndSize(t *testing.T) {
	big := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte(strings.Repeat("x", proxyMaxBytes+1)))
	}))
	defer big.Close()
	slow := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		select {
		case <-r.Context().Done():
		case <-time.After(2 * time.Second):
		}
	}))
	defer slow.Close()
	s := &server{dist: t.TempDir(), rtProxy: true, proxyTimeout: 100 * time.Millisecond}
	srv := httptest.NewServer(s.routes())
	defer srv.Close()
	resp, _ := http.Get(srv.URL + "/rt?url=" + url.QueryEscape(big.URL))
	resp.Body.Close()
	if resp.StatusCode != http.StatusBadGateway {
		t.Fatalf("an upstream past the size cap is refused: %d", resp.StatusCode)
	}
	resp, _ = http.Get(srv.URL + "/rt?url=" + url.QueryEscape(slow.URL))
	resp.Body.Close()
	if resp.StatusCode != http.StatusBadGateway {
		t.Fatalf("an upstream past the timeout is refused: %d", resp.StatusCode)
	}
}
