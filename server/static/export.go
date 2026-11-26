package static

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"log"
	"os"
	"path/filepath"
	"sort"

	"rustwasm/server/imports"
)

// The static bundle uses the paths that the server answers, so a page
// reads either deployment through one layout.

// Export requires dir to be absent or empty. An export never writes
// over files, because uploading a bundle keeps every blob that an
// earlier one wrote, so an open page pinned to the old manifest still
// finds its documents. liveFeeds, when not "", replaces -dist's copy of
// live-feeds.json.
// Export writes the static bundle: the pages of dist, the manifest, and
// each published document as blobs/<hash> and blobs/<hash>.gz.
func Export(dir, dist, liveFeeds, feedURL string, manifest []byte, docs []imports.Doc) error {
	if entries, err := os.ReadDir(dir); err == nil && len(entries) > 0 {
		return fmt.Errorf("%s is not empty: an export writes a fresh bundle", dir)
	} else if err != nil && !errors.Is(err, fs.ErrNotExist) {
		return err
	}
	if err := CopyTree(dist, dir); err != nil {
		return fmt.Errorf("copy the pages from %s: %w", dist, err)
	}

	if err := os.WriteFile(filepath.Join(dir, "manifest"), manifest, 0o644); err != nil {
		return err
	}
	blobs := filepath.Join(dir, "blobs")
	if err := os.MkdirAll(blobs, 0o755); err != nil {
		return err
	}
	for _, doc := range docs {
		if err := os.WriteFile(filepath.Join(blobs, doc.Hash), doc.Body, 0o644); err != nil {
			return err
		}
		if err := os.WriteFile(filepath.Join(blobs, doc.Hash+".gz"), doc.Gz, 0o644); err != nil {
			return err
		}
	}

	if liveFeeds != "" {
		feeds, err := os.ReadFile(liveFeeds)
		if err != nil {
			return fmt.Errorf("-live-feeds: %w", err)
		}
		if err := os.WriteFile(filepath.Join(dir, "live-feeds.json"), feeds, 0o644); err != nil {
			return err
		}
	}
	if feeds, err := os.ReadFile(filepath.Join(dir, "live-feeds.json")); err == nil {
		var config struct {
			Proxy string `json:"proxy"`
		}
		if json.Unmarshal(feeds, &config) == nil && config.Proxy != "" {
			log.Printf("export: live-feeds.json names the relay %q, which a static host does not serve — pass the deployment's own with -live-feeds", config.Proxy)
		}
	}
	var pages []string
	for page := range riderPages {
		if _, err := os.Stat(filepath.Join(dir, page)); err == nil {
			pages = append(pages, page)
		}
	}
	sort.Strings(pages)
	live, _ := os.ReadFile(filepath.Join(dir, "live-feeds.json"))
	rules := DeploymentRules(pages, FeedOrigins(live, feedURL))
	if err := os.WriteFile(filepath.Join(dir, "_headers"), []byte(RenderHeadersFile(rules)), 0o644); err != nil {
		return err
	}
	log.Printf("export: %s — manifest, %d documents, the pages from %s", dir, len(docs), dist)
	return nil
}

// CopyTree copies the regular files under src into dst.
func CopyTree(src, dst string) error {
	return filepath.WalkDir(src, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(src, path)
		if err != nil {
			return err
		}
		target := filepath.Join(dst, rel)
		if d.IsDir() {
			return os.MkdirAll(target, 0o755)
		}
		if !d.Type().IsRegular() {
			return nil
		}
		in, err := os.Open(path)
		if err != nil {
			return err
		}
		defer in.Close()
		out, err := os.Create(target)
		if err != nil {
			return err
		}
		if _, err := io.Copy(out, in); err != nil {
			out.Close()
			return err
		}
		return out.Close()
	})
}
