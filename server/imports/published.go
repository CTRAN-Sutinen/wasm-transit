// Package imports derives the server's documents from its inputs: the
// feed's network and boundary, and the street layer's streets and walk
// table (docs/architecture/server.md; docs/architecture/storage.md,
// "Derived documents"). Each input is read once per change of its bytes,
// in a fresh instance closed after. With a store, each derivation is
// recorded, so a boot whose inputs the store already derived from runs no
// module. Published serves the documents at their stable routes.
package imports

// Published serves the documents derived from the inputs that are
// configured. A nil source answers 404.
type Published struct {
	Feed    *FeedSource
	Streets *StreetsSource
}

// Blob is a published document by its hash. In store mode the store holds
// these documents too, and /blobs answers the store's blob first.
func (p *Published) Blob(hash string) (Doc, bool) {
	docs, err := p.Docs()
	if err != nil {
		return Doc{}, false
	}
	for _, doc := range docs {
		if doc.Hash == hash {
			return doc, true
		}
	}
	return Doc{}, false
}

// Docs are the documents the server publishes from its inputs.
func (p *Published) Docs() ([]Doc, error) {
	var docs []Doc
	if p.Feed != nil {
		feed, err := p.Feed.Load()
		if err != nil {
			return nil, err
		}
		docs = append(docs,
			Doc{Hash: feed.NetworkHash, Body: feed.Document, Gz: feed.DocumentGzip, Tag: "network"},
			Doc{Hash: feed.BoundaryHash, Body: feed.Boundary, Gz: feed.BoundaryGzip, Tag: "boundary"})
	}
	if p.Streets != nil {
		streets, err := p.PublishedStreets()
		if err != nil {
			return nil, err
		}
		doc := streets.Streets
		doc.Tag = "streets"
		docs = append(docs, doc)
		if streets.Walks.Hash != "" {
			walks := streets.Walks
			walks.Tag = "walks"
			docs = append(docs, walks)
		}
	}
	return docs, nil
}
