package main

// The headers a deployment's pages must send (docs/architecture/
// static-deployment.md, "Headers"; docs/architecture/server.md, "Static
// files"). One set of rules serves both shapes: the server sends them
// on its static files, and `-export` writes them as the bundle's
// `_headers` file (Cloudflare's and Netlify's format), so a static host
// sends the same.
//
//   - Every file: X-Content-Type-Options nosniff, and a Referrer-Policy.
//   - Each rider page (ride-now.html, stop-display.html): a
//     Content-Security-Policy and a Permissions-Policy. The policy names
//     the origins the page reads: itself, the agency's live feeds
//     (live-feeds.json), the agency's GTFS URL (-feed-url), and, for the
//     page that draws a map, the tile hosts.
//   - Compression: the server gzips the text and wasm types when the
//     client takes it (gzipStatic). A static host does it itself.

import (
	"compress/gzip"
	"encoding/json"
	"mime"
	"net/http"
	"net/url"
	"sort"
	"strings"
)

// The rider pages, and whether the page draws a map and reads the
// rider's location. The stop display does neither.
var riderPages = map[string]struct{ map_, geolocation bool }{
	"ride-now.html":     {map_: true, geolocation: true},
	"stop-display.html": {},
}

// The basemap archive has no registered type, so ServeContent would
// sniff one from its first bytes, and a start that looks like text
// would be gzipped. Name it: it is compressed already, and it is read
// by ranges.
func init() {
	mime.AddExtensionType(".pmtiles", "application/vnd.pmtiles")
}

func isRiderPage(name string) bool {
	_, ok := riderPages[name]
	return ok
}

// The basemap's remote hosts (web/src/basemap.ts): OpenFreeMap's tiles
// and style, and Protomaps' glyphs and sprites. A page that has its own
// basemap.pmtiles and assets reads none of them, but the page chooses at
// run time, so the policy allows both.
var tileOrigins = []string{"https://tiles.openfreemap.org", "https://protomaps.github.io"}

// A header, by name and value.
type header struct{ name, value string }

// headerRule is the headers for the paths a pattern matches. `*` in a
// pattern matches any characters, as in a `_headers` file.
type headerRule struct {
	pattern string
	headers []header
}

// originOf is the scheme and host of an absolute http(s) URL; "" for a
// relative or other URL.
func originOf(raw string) string {
	u, err := url.Parse(raw)
	if err != nil || (u.Scheme != "http" && u.Scheme != "https") || u.Host == "" {
		return ""
	}
	return u.Scheme + "://" + u.Host
}

// feedOrigins is every origin a live-feeds.json names, with the origin
// of the agency's GTFS URL (feedURL, which may be ""). A relative
// proxy ("/rt") is the page's own origin, which the policy has.
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

// contentSecurityPolicy is the policy of one rider page. `connect` is
// the origins the page fetches beyond its own. The modules need
// 'wasm-unsafe-eval' (and nothing else that evaluates code); the bus
// worker and the service worker are the page's own scripts; the map's
// worker is a blob.
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

// permissionsPolicy turns off every powerful feature a rider page does
// not use. Ride now allows the rider's location for itself; the stop
// display allows none.
func permissionsPolicy(page string) string {
	geolocation := "()"
	if riderPages[page].geolocation {
		geolocation = "(self)"
	}
	return "geolocation=" + geolocation + ", camera=(), microphone=(), payment=(), usb=()"
}

// baselineHeaders go on every file.
func baselineHeaders() []header {
	return []header{
		{"X-Content-Type-Options", "nosniff"},
		{"Referrer-Policy", "strict-origin-when-cross-origin"},
	}
}

// pageHeaders go on one rider page, beside the baseline. nil for a file
// that is not a rider page.
func pageHeaders(page string, connect []string) []header {
	if !isRiderPage(page) {
		return nil
	}
	return []header{
		{"Content-Security-Policy", contentSecurityPolicy(page, connect)},
		{"Permissions-Policy", permissionsPolicy(page)},
	}
}

// immutableCache is the rule for a file named by its content hash.
const immutableCache = "public, max-age=31536000, immutable"

// deploymentRules is the whole `_headers` file of an exported bundle:
// the baseline, the caching of the files named by their hash, the
// manifest's type, and each rider page the bundle holds (pages).
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

// renderHeadersFile writes rules in the `_headers` format: a path
// pattern on its own line, then each header indented under it.
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

// compressible says which content types the server gzips: text, and
// the wasm modules. Never application/gzip (the documents, which the
// modules decode themselves) and never a type that is compressed
// already (images, the pmtiles archive).
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

// gzipWriter compresses a response whose type is compressible. The
// decision waits for the header, so the handler picks the type first.
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
	// The bytes differ from the stored file's: a strong ETag must not
	// name both.
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

// gzipStatic runs serve with compression when the client takes gzip.
// A Range request (the basemap's pmtiles) and a HEAD request are
// answered as they are: a range is a range of the stored bytes.
func gzipStatic(w http.ResponseWriter, r *http.Request, serve func(http.ResponseWriter, *http.Request)) {
	if r.Method != http.MethodGet || r.Header.Get("Range") != "" || !strings.Contains(r.Header.Get("Accept-Encoding"), "gzip") {
		serve(w, r)
		return
	}
	g := &gzipWriter{ResponseWriter: w}
	defer g.close()
	serve(g, r)
}
