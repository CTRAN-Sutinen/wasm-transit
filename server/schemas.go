package main

// The documents' schemas at /schemas/ (docs/FORMATS.md): the index
// documents.json and each version's schema and example, which every
// page build carries (gen-manifest writes them into web/public/schemas/).
// They are ordinary build files, open with sign-in on (reads are open,
// D-152), so a rollback serves the schemas of the build it moves to. A
// blob's response links its schema. The index holds each Link value
// (`links`), so the server copies one and decides nothing.

import (
	"encoding/json"
	"fmt"
)

// schemaLinks is the `links` of /schemas/documents.json.
type schemaLinks struct {
	// Stored is by a stored blob's envelope: "<kind> v<N>".
	Stored map[string]string `json:"stored"`
	// Published is by a published document's tag.
	Published map[string]string `json:"published"`
}

// links reads the pinned build's (or -dist's) schema links. Empty when
// the build has no index.
func (s *server) links() schemaLinks {
	var index struct {
		Links schemaLinks `json:"links"`
	}
	if raw := s.staticBytes("schemas/documents.json"); raw != nil {
		json.Unmarshal(raw, &index)
	}
	return index.Links
}

// storedSchemaLink is the Link header of a stored blob, by its envelope.
func (s *server) storedSchemaLink(kind string, version uint32) string {
	return s.links().Stored[fmt.Sprintf("%s v%d", kind, version)]
}

// publishedSchemaLink is the Link header of a published document, by
// its tag.
func (s *server) publishedSchemaLink(tag string) string {
	return s.links().Published[tag]
}
