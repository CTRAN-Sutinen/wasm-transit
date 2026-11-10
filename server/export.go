package main

// The static bundle (D-131; docs/ARCHITECTURE.md, server section,
// "Static deployment"). `-export DIR` boots as the server does — the
// same modules import the feed and the street layer and derive the walk
// table — then writes the deployment as files and exits:
//
//	DIR/<the pages>         -dist, byte for byte
//	DIR/manifest            the manifest the server answers at /manifest
//	DIR/blobs/<hash>        each published document's JSON (sha256 = hash)
//	DIR/blobs/<hash>.gz     the same bytes gzipped
//	DIR/live-feeds.json     -live-feeds when given, else -dist's copy
//
// The server answers the same paths (/manifest, /blobs/<hash>[.gz] —
// pointers.go), so a page reads either deployment through one layout.
// The export is the server's publishing without the serving: it adds
// nothing a running server does not answer.

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"log"
	"os"
	"path/filepath"
)

// publishedBlob answers a published document by its hash — the
// network, its boundary, the streets, the walk table — as served
// (servedDoc: its bytes, and gzipped). False when no published document
// has that hash.
func (s *server) publishedBlob(hash string) (servedDoc, bool) {
	docs, err := s.publishedDocs()
	if err != nil {
		return servedDoc{}, false
	}
	for _, doc := range docs {
		if doc.hash == hash {
			return doc, true
		}
	}
	return servedDoc{}, false
}

// publishedDocs lists the documents the manifest pins, each under the
// hash it pins.
func (s *server) publishedDocs() ([]servedDoc, error) {
	var docs []servedDoc
	if s.feeder != nil {
		feed, err := s.feeder.Load()
		if err != nil {
			return nil, err
		}
		docs = append(docs,
			servedDoc{hash: feed.networkHash, body: feed.document, gz: feed.documentGzip},
			servedDoc{hash: feed.boundaryHash, body: feed.boundary, gz: feed.boundaryGzip})
	}
	if s.streets != nil {
		streets, err := s.publishedStreets()
		if err != nil {
			return nil, err
		}
		docs = append(docs, streets.streets)
		if streets.walks.hash != "" {
			docs = append(docs, streets.walks)
		}
	}
	return docs, nil
}

// export writes the static bundle into dir, which must be absent or
// empty — an export never writes over files (uploading a bundle keeps
// every blob an earlier one wrote, so an open page pinned to the old
// manifest still finds its documents). liveFeeds, when not "", is the
// deployment's live-feeds.json, replacing -dist's dev copy.
func (s *server) export(dir, liveFeeds string) error {
	if s.store != nil {
		return errors.New("-export publishes the configured feed and streets; it does not export a store's picks (run it without -store)")
	}
	if entries, err := os.ReadDir(dir); err == nil && len(entries) > 0 {
		return fmt.Errorf("%s is not empty: an export writes a fresh bundle", dir)
	} else if err != nil && !errors.Is(err, fs.ErrNotExist) {
		return err
	}
	if err := copyTree(s.dist, dir); err != nil {
		return fmt.Errorf("copy the pages from %s: %w", s.dist, err)
	}

	manifest, err := s.buildManifest()
	if err != nil {
		return err
	}
	body, err := json.Marshal(manifest)
	if err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join(dir, "manifest"), body, 0o644); err != nil {
		return err
	}

	docs, err := s.publishedDocs()
	if err != nil {
		return err
	}
	blobs := filepath.Join(dir, "blobs")
	if err := os.MkdirAll(blobs, 0o755); err != nil {
		return err
	}
	for _, doc := range docs {
		if err := os.WriteFile(filepath.Join(blobs, doc.hash), doc.body, 0o644); err != nil {
			return err
		}
		if err := os.WriteFile(filepath.Join(blobs, doc.hash+".gz"), doc.gz, 0o644); err != nil {
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
			log.Printf("export: live-feeds.json names the relay %q, which a static host does not serve — pass the deployment's own with -live-feeds (D-67)", config.Proxy)
		}
	}
	log.Printf("export: %s — manifest, %d documents, the pages from %s", dir, len(docs), s.dist)
	return nil
}

// copyTree copies every regular file under src to the same path under
// dst.
func copyTree(src, dst string) error {
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
