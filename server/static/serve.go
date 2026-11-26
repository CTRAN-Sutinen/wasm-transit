package static

import (
	"net/http"
	"os"
	"path"
	"path/filepath"
	"strings"

	"rustwasm/server/builds"
	"rustwasm/server/store"
)

// Static serves the pages.
type Static struct {
	store   func() *store.Store
	dist    string
	builds  *builds.Builds
	feedURL string
}

// New serves the pinned build with a store, else -dist (dist). feedURL is
// the GTFS URL the pages fetch, "" when the server imports its own feed.
func New(st func() *store.Store, dist string, b *builds.Builds, feedURL string) *Static {
	return &Static{store: st, dist: dist, builds: b, feedURL: feedURL}
}

// HandleStatic serves the pages with the deployment's headers.
func (st *Static) HandleStatic(w http.ResponseWriter, r *http.Request) {
	h := w.Header()
	for _, b := range baselineHeaders() {
		h.Set(b.name, b.value)
	}
	if page := strings.TrimPrefix(path.Clean(r.URL.Path), "/"); isRiderPage(page) {
		for _, p := range pageHeaders(page, st.connectOrigins()) {
			h.Set(p.name, p.value)
		}
	}
	Gzip(w, r, st.serveStatic)
}

func (st *Static) connectOrigins() []string {
	return FeedOrigins(st.Bytes("live-feeds.json"), st.feedURL)
}

func (st *Static) Bytes(name string) []byte {
	if st.store() != nil {
		if _, files := st.builds.PinnedFiles(); files != nil {
			if f, ok := files[name]; ok {
				if data, _, err := st.store().GetBlob(f.Hash); err == nil {
					return data
				}
			}
		}
		return nil
	}
	data, _ := os.ReadFile(filepath.Join(st.dist, name))
	return data
}

// http.ServeContent gives the Range requests that PMTiles needs.
func (st *Static) serveStatic(w http.ResponseWriter, r *http.Request) {
	upath := r.URL.Path
	if strings.Contains(upath, "..") {
		http.Error(w, "bad path", http.StatusBadRequest)
		return
	}
	name := path.Clean(strings.TrimPrefix(upath, "/"))
	if name == "" || name == "." {
		name = "index.html"
	}
	// In store mode the pages come from the pinned build (server/builds).
	// -dist supplies what the build lacks, such as the basemap.
	if st.store() != nil {
		st.builds.Sync()
		if st.builds.ServeFile(w, r, name) {
			return
		}
	}
	file, err := os.Open(filepath.Join(st.dist, name))
	if err != nil {
		http.NotFound(w, r)
		return
	}
	defer file.Close()
	info, err := file.Stat()
	if err != nil || info.IsDir() {
		http.NotFound(w, r)
		return
	}
	w.Header().Set("Cache-Control", builds.CacheControl(name))
	http.ServeContent(w, r, info.Name(), info.ModTime(), file)
}
