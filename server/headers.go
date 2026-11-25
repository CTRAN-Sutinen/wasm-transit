package main

// The headers a deployment's pages must send (docs/architecture/
// static-deployment.md, "Headers"). The server sends them on its static
// files, and `-export` writes them as the bundle's `_headers` file, so a
// static host sends the same.

import (
	"compress/gzip"
	"encoding/json"
	"mime"
	"net/http"
	"net/url"
	"sort"
	"strings"
)

var riderPages = map[string]struct{ map_, geolocation bool }{
	"ride-now.html":     {map_: true, geolocation: true},
	"stop-display.html": {},
}

// ServeContent would sniff a type for the basemap archive from its first
// bytes, and a start that looks like text would be gzipped. The archive
// is compressed already and read by ranges.
func init() {
	mime.AddExtensionType(".pmtiles", "application/vnd.pmtiles")
}

func isRiderPage(name string) bool {
	_, ok := riderPages[name]
	return ok
}

// The remote basemap hosts (web/src/basemap.ts). A page with its own
// basemap.pmtiles reads none of them, but it chooses at run time, so the
// policy allows both.
var tileOrigins = []string{"https://tiles.openfreemap.org", "https://protomaps.github.io"}

type header struct{ name, value string }

// A `*` in pattern matches any characters, as in a `_headers` file.
type headerRule struct {
	pattern string
	headers []header
}

// originOf answers "" for a relative or non-http(s) URL.
func originOf(raw string) string {
	u, err := url.Parse(raw)
	if err != nil || (u.Scheme != "http" && u.Scheme != "https") || u.Host == "" {
		return ""
	}
	return u.Scheme + "://" + u.Host
}

// feedOrigins leaves out a relative proxy such as "/rt", because it is
// the page's own origin.
func feedOrigins(liveFeeds []byte, feedURL string) []string {
	seen := map[string]bool{}
	var config map[string]any
	if json.Unmarshal(liveFeeds, &config) == nil {
		for _, value := range config {
			if s, ok := value.(string); ok {
				if o := originOf(s); o != "" {
					seen[o] = true
				}
			}
		}
	}
	if o := originOf(feedURL); o != "" {
		seen[o] = true
	}
	origins := make([]string, 0, len(seen))
	for o := range seen {
		origins = append(origins, o)
	}
	sort.Strings(origins)
	return origins
}

// The modules need 'wasm-unsafe-eval' and nothing else that evaluates
// code. The map's worker is a blob. `connect` is the origins the page
// fetches beyond its own.
func contentSecurityPolicy(page string, connect []string) string {
	sources := append([]string{"'self'"}, connect...)
	if riderPages[page].map_ {
		sources = append(sources, tileOrigins...)
	}
	return strings.Join([]string{
		"default-src 'none'",
		"script-src 'self' 'wasm-unsafe-eval'",
		"style-src 'self'",
		"img-src 'self' data: blob:",
		"font-src 'self'",
		"connect-src " + strings.Join(sources, " "),
		"worker-src 'self' blob:",
		"manifest-src 'self'",
		"base-uri 'none'",
		"form-action 'none'",
		"frame-ancestors 'none'",
		"object-src 'none'",
	}, "; ")
}

func permissionsPolicy(page string) string {
	geolocation := "()"
	if riderPages[page].geolocation {
		geolocation = "(self)"
	}
	return "geolocation=" + geolocation + ", camera=(), microphone=(), payment=(), usb=()"
}

func baselineHeaders() []header {
	return []header{
		{"X-Content-Type-Options", "nosniff"},
		{"Referrer-Policy", "strict-origin-when-cross-origin"},
	}
}

func pageHeaders(page string, connect []string) []header {
	if !isRiderPage(page) {
		return nil
	}
	return []header{
		{"Content-Security-Policy", contentSecurityPolicy(page, connect)},
		{"Permissions-Policy", permissionsPolicy(page)},
	}
}

const immutableCache = "public, max-age=31536000, immutable"

// Every other file revalidates on each use, which a static host does by
// default, so it needs no rule.
func deploymentRules(pages []string, connect []string) []headerRule {
	rules := []headerRule{
		{"/*", baselineHeaders()},
		{"/assets/*", []header{{"Cache-Control", immutableCache}}},
		{"/caps/*", []header{{"Cache-Control", immutableCache}}},
		{"/blobs/*", []header{{"Cache-Control", immutableCache}}},
		{"/manifest", []header{{"Content-Type", "application/json"}}},
	}
	for _, page := range pages {
		if h := pageHeaders(page, connect); h != nil {
			rules = append(rules, headerRule{"/" + page, h})
		}
	}
	return rules
}

func renderHeadersFile(rules []headerRule) string {
	var b strings.Builder
	b.WriteString("# Written by the export (server/headers.go). Upload it with the bundle.\n")
	for _, r := range rules {
		b.WriteString(r.pattern + "\n")
		for _, h := range r.headers {
			b.WriteString("  " + h.name + ": " + h.value + "\n")
		}
	}
	return b.String()
}

// compressible excludes application/gzip, because the modules decode the
// documents themselves, and types that are compressed already.
func compressible(contentType string) bool {
	t := strings.ToLower(strings.TrimSpace(strings.SplitN(contentType, ";", 2)[0]))
	switch {
	case strings.HasPrefix(t, "text/"):
		return true
	case t == "application/javascript", t == "application/json", t == "application/wasm",
		t == "application/manifest+json", t == "application/xml", t == "image/svg+xml":
		return true
	}
	return false
}

type gzipWriter struct {
	http.ResponseWriter
	zw      *gzip.Writer
	decided bool
}

func (g *gzipWriter) decide(code int) {
	g.decided = true
	h := g.Header()
	if code != http.StatusOK || h.Get("Content-Encoding") != "" || !compressible(h.Get("Content-Type")) {
		return
	}
	h.Set("Content-Encoding", "gzip")
	h.Add("Vary", "Accept-Encoding")
	h.Del("Content-Length")
	// A strong ETag must not name both the stored bytes and these.
	if etag := h.Get("ETag"); etag != "" && !strings.HasPrefix(etag, "W/") {
		h.Set("ETag", "W/"+etag)
	}
	g.zw, _ = gzip.NewWriterLevel(g.ResponseWriter, 5)
}

func (g *gzipWriter) WriteHeader(code int) {
	if !g.decided {
		g.decide(code)
	}
	g.ResponseWriter.WriteHeader(code)
}

func (g *gzipWriter) Write(p []byte) (int, error) {
	if !g.decided {
		g.decide(http.StatusOK)
	}
	if g.zw != nil {
		return g.zw.Write(p)
	}
	return g.ResponseWriter.Write(p)
}

func (g *gzipWriter) close() {
	if g.zw != nil {
		g.zw.Close()
	}
}

// gzipStatic answers a Range request and a HEAD request uncompressed,
// because a range is a range of the stored bytes.
func gzipStatic(w http.ResponseWriter, r *http.Request, serve func(http.ResponseWriter, *http.Request)) {
	if r.Method != http.MethodGet || r.Header.Get("Range") != "" || !strings.Contains(r.Header.Get("Accept-Encoding"), "gzip") {
		serve(w, r)
		return
	}
	g := &gzipWriter{ResponseWriter: w}
	defer g.close()
	serve(g, r)
}
