package main

import (
	"encoding/json"
	"errors"

	"rustwasm/server/static"
)

// export writes the static bundle (static.Export) of the configured feed
// and streets.
func (s *server) export(dir, liveFeeds string) error {
	if s.store != nil {
		return errors.New("-export publishes the configured feed and streets; it does not export a store's picks (run it without -store)")
	}
	manifest, err := s.buildManifest()
	if err != nil {
		return err
	}
	body, err := json.Marshal(manifest)
	if err != nil {
		return err
	}
	docs, err := s.published().Docs()
	if err != nil {
		return err
	}
	return static.Export(dir, s.dist, liveFeeds, s.feedURL, body, docs)
}
