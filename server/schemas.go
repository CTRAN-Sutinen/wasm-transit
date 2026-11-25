package main

// The schemas at /schemas/ are ordinary build files (docs/FORMATS.md), so
// a rollback serves the schemas of the build it moves to. The index holds
// each Link value (`links`), so the server only copies one.

import (
	"encoding/json"
	"fmt"
)

type schemaLinks struct {
	// Stored is keyed "<kind> v<N>", from a blob's envelope.
	Stored map[string]string `json:"stored"`
	// Published is keyed by a published document's tag.
	Published map[string]string `json:"published"`
}

// links is empty when the build has no index.
func (s *server) links() schemaLinks {
	var index struct {
		Links schemaLinks `json:"links"`
	}
	if raw := s.staticBytes("schemas/documents.json"); raw != nil {
		json.Unmarshal(raw, &index)
	}
	return index.Links
}

func (s *server) storedSchemaLink(kind string, version uint32) string {
	return s.links().Stored[fmt.Sprintf("%s v%d", kind, version)]
}

func (s *server) publishedSchemaLink(tag string) string {
	return s.links().Published[tag]
}
