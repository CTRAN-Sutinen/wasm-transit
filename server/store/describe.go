package store

// The store describes itself (D-203; docs/FORMATS.md, "The store
// describes itself"). Each serving process creates, create only, what
// an outside reader needs to read the bucket with no app:
//
//	layout                              the layout's version and hash scheme
//	README                              the layout in plain language
//	kinds/<hex>.json                    each build's kind table, by content hash
//	schemas/<tag>.v<N>/<hex>.json       each locked schema, by content hash
//
// A field whose schema says `"format": "content-hash"` names a blob in
// this store. `store verify` and `store gc` follow those references:
// from every blob a record names, and from every record in every log
// entry (a log record's schema is its document's, through the kind
// table's records). A store with none of these objects is read as
// before: the records and the builds name what gc keeps.

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"sort"
	"strconv"
	"strings"
)

// LayoutKind is the layout object's kind; LayoutVersion the layout it
// describes.
const (
	LayoutKind    = "store_layout"
	LayoutVersion = 1
)

// Layout is the `layout` object.
type Layout struct {
	Kind          string `json:"kind"`
	SchemaVersion int    `json:"schema_version"`
	// Hash is the scheme every content hash in the store uses: "sha256",
	// written "sha256:<hex>".
	Hash string `json:"hash"`
	// Readme is the key of the plain-language description.
	Readme string `json:"readme"`
}

// LayoutKey and ReadmeKey are the layout objects' keys.
const (
	LayoutKey = "layout"
	ReadmeKey = "README"
)

// Readme is the README a store carries: the layout in plain words, for a
// person with only the bucket.
const Readme = `This bucket is a transit agency's record, written by wasm-transit.
Every object is written once and never changed.

layout
    The layout's version and how content is hashed (sha256).
blobs/<hex>
    One document, its exact bytes. <hex> is the sha256 of the bytes.
    Its metadata "envelope" names its kind, its version, and the
    module that made it.
pointers/<name>/<seq>.json
    One commit of a named document. The highest seq is the current
    one. Each commit names the blob ("to"), the one before it ("from"),
    who wrote it, and when.
logs/<log>/entries/<seq>.json
    One change to a log of records (the disruptions, the stops, the
    plans): each record before and after, who changed it, and why.
logs/<log>/checkpoints/<seq>.json
    The records of a log up to an entry, so a reader need not read
    every entry.
schemas/<tag>.v<N>/<hex>.json
    The JSON Schema of each kind and version, by its sha256. A
    document names its kind ("kind") and version ("schema_version")
    inside it. A field whose format is "content-hash" names a blob.
kinds/<hex>.json
    Each build's kind table: how each kind is written and read, and
    which log records belong to which document.
gc/<hex>
    A blob the collector is moving out. It is empty between passes.

The formats are described in full in docs/FORMATS.md of wasm-transit.
`

// Describe creates the store's description, create only: the layout and
// the README once, the kind table, and every schema it names (schemas:
// "sha256:<hex>" → bytes). Each object that exists already is left as it
// is. A schema whose bytes do not hash to its name is refused.
func (s *Store) Describe(table []byte, schemas map[string][]byte) error {
	ctx := context.Background()
	layout, err := json.Marshal(Layout{Kind: LayoutKind, SchemaVersion: LayoutVersion, Hash: "sha256", Readme: ReadmeKey})
	if err != nil {
		return err
	}
	var t describedTable
	if err := json.Unmarshal(table, &t); err != nil {
		return fmt.Errorf("the kind table: %w", err)
	}
	objects := []struct {
		key  string
		data []byte
	}{{LayoutKey, layout}, {ReadmeKey, []byte(Readme)}, {kindsKey(hashOf(table)), table}}
	for _, k := range t.Kinds {
		for _, v := range k.Versions {
			data, ok := schemas[v.Schema]
			if !ok {
				return fmt.Errorf("the kind table names the %s v%d schema %s, and it is not given", k.Tag, v.Version, v.Schema)
			}
			if hashOf(data) != v.Schema {
				return fmt.Errorf("the %s v%d schema hashes to %s, not %s", k.Tag, v.Version, hashOf(data), v.Schema)
			}
			objects = append(objects, struct {
				key  string
				data []byte
			}{schemaKey(k.Tag, v.Version, v.Schema), data})
		}
	}
	for _, o := range objects {
		if _, err := s.b.Stat(ctx, o.key); err == nil {
			continue
		} else if !errors.Is(err, ErrNotFound) {
			return err
		}
		if err := s.b.Create(ctx, o.key, o.data, nil); err != nil && !errors.Is(err, ErrExists) {
			return err
		}
	}
	return nil
}

func kindsKey(hash string) string { return "kinds/" + strings.TrimPrefix(hash, "sha256:") + ".json" }

func schemaKey(tag string, version uint32, hash string) string {
	return fmt.Sprintf("schemas/%s.v%d/%s.json", tag, version, strings.TrimPrefix(hash, "sha256:"))
}

// describedTable is the part of a kind table the store reads.
type describedTable struct {
	Kinds []struct {
		StoreKind string `json:"store_kind"`
		Tag       string `json:"tag"`
		Versions  []struct {
			Version uint32 `json:"version"`
			Schema  string `json:"schema"`
		} `json:"versions"`
		Records []struct {
			Kind       string `json:"kind"`
			Collection string `json:"collection"`
		} `json:"records"`
	} `json:"kinds"`
}

// recordPlace is where a log record lives: its document's tag, and the
// document field that holds the collection.
type recordPlace struct {
	tag        string
	collection string
}

// described is what the store says of itself: each kind's schemas, and
// where each log record kind lives.
type described struct {
	tagOf   map[string]string           // store kind → tag
	schemas map[string][]map[string]any // "<tag> v<N>" → its schemas (an additive change keeps one version)
	records map[string]recordPlace      // record kind → its place
}

// readDescribed reads the store's kind tables and schemas. A store with
// none answers an empty description.
func readDescribed(ctx context.Context, b Backend) (*described, error) {
	d := &described{tagOf: map[string]string{}, schemas: map[string][]map[string]any{}, records: map[string]recordPlace{}}
	tables, err := b.List(ctx, "kinds/", "")
	if err != nil {
		return nil, err
	}
	for _, o := range tables {
		data, _, err := b.Get(ctx, o.Key)
		if err != nil {
			return nil, err
		}
		var t describedTable
		if err := json.Unmarshal(data, &t); err != nil {
			return nil, fmt.Errorf("%s: %w", o.Key, err)
		}
		for _, k := range t.Kinds {
			if k.Tag != "" {
				d.tagOf[k.StoreKind] = k.Tag
			}
			for _, r := range k.Records {
				d.records[r.Kind] = recordPlace{tag: k.Tag, collection: r.Collection}
			}
		}
	}
	objs, err := b.List(ctx, "schemas/", "")
	if err != nil {
		return nil, err
	}
	for _, o := range objs {
		rest := strings.TrimPrefix(o.Key, "schemas/")
		dir, _, ok := strings.Cut(rest, "/")
		dot := strings.LastIndex(dir, ".v")
		if !ok || dot < 0 {
			continue
		}
		version, err := strconv.ParseUint(dir[dot+2:], 10, 32)
		if err != nil {
			continue
		}
		data, _, err := b.Get(ctx, o.Key)
		if err != nil {
			return nil, err
		}
		var schema map[string]any
		if err := json.Unmarshal(data, &schema); err != nil {
			return nil, fmt.Errorf("%s: %w", o.Key, err)
		}
		key := fmt.Sprintf("%s v%d", dir[:dot], version)
		d.schemas[key] = append(d.schemas[key], schema)
	}
	return d, nil
}

// marks says whether a blob of the store kind and version may name a
// blob: one of its schemas marks a field.
func (d *described) marks(kind string, version uint32) bool {
	for _, s := range d.schemas[fmt.Sprintf("%s v%d", d.tagOf[kind], version)] {
		if hasMark(s) {
			return true
		}
	}
	return false
}

func hasMark(node any) bool {
	switch n := node.(type) {
	case map[string]any:
		if n["format"] == "content-hash" {
			return true
		}
		for _, v := range n {
			if hasMark(v) {
				return true
			}
		}
	case []any:
		for _, v := range n {
			if hasMark(v) {
				return true
			}
		}
	}
	return false
}

// refs answers the hex of every blob a document of the store kind and
// version names, by its schemas' marks. A document that is not JSON
// names nothing.
func (d *described) refs(kind string, version uint32, doc []byte) []string {
	schemas := d.schemas[fmt.Sprintf("%s v%d", d.tagOf[kind], version)]
	if len(schemas) == 0 {
		return nil
	}
	var v any
	dec := json.NewDecoder(bytes.NewReader(doc))
	if err := dec.Decode(&v); err != nil {
		return nil
	}
	found := map[string]bool{}
	for _, s := range schemas {
		walk(s, s, v, found, 0)
	}
	return sortedKeys(found)
}

// recordRefs answers the hex of every blob a log record names: the
// record walked by the schema of its document's collection item.
func (d *described) recordRefs(kind string, version uint32, record json.RawMessage) []string {
	place, ok := d.records[kind]
	if !ok || len(record) == 0 {
		return nil
	}
	var v any
	if err := json.Unmarshal(record, &v); err != nil {
		return nil
	}
	found := map[string]bool{}
	for _, s := range d.schemas[fmt.Sprintf("%s v%d", place.tag, version)] {
		props, _ := s["properties"].(map[string]any)
		col, _ := props[place.collection].(map[string]any)
		col = resolve(s, col)
		for _, key := range []string{"items", "additionalProperties"} {
			if item, ok := col[key].(map[string]any); ok {
				walk(s, item, v, found, 0)
			}
		}
	}
	return sortedKeys(found)
}

func sortedKeys(m map[string]bool) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	sort.Strings(out)
	return out
}

// resolve follows a local $ref (#/$defs/<name> or #/definitions/<name>).
func resolve(root, node map[string]any) map[string]any {
	for i := 0; i < 16 && node != nil; i++ {
		ref, ok := node["$ref"].(string)
		if !ok {
			return node
		}
		path := strings.Split(strings.TrimPrefix(ref, "#/"), "/")
		var at any = root
		for _, p := range path {
			m, _ := at.(map[string]any)
			at = m[p]
		}
		next, _ := at.(map[string]any)
		if next == nil {
			return node
		}
		node = next
	}
	return node
}

// walk collects the hex of each string at a node whose format is
// content-hash, through properties, additionalProperties, items, and
// every branch of allOf, anyOf, and oneOf.
func walk(root, node map[string]any, v any, found map[string]bool, depth int) {
	if node == nil || depth > 64 {
		return
	}
	node = resolve(root, node)
	if node["format"] == "content-hash" {
		if s, ok := v.(string); ok {
			if h, err := ParseHash(s); err == nil {
				found[h] = true
			}
		}
	}
	for _, k := range []string{"allOf", "anyOf", "oneOf"} {
		if branches, ok := node[k].([]any); ok {
			for _, b := range branches {
				if m, ok := b.(map[string]any); ok {
					walk(root, m, v, found, depth+1)
				}
			}
		}
	}
	switch val := v.(type) {
	case map[string]any:
		props, _ := node["properties"].(map[string]any)
		extra, _ := node["additionalProperties"].(map[string]any)
		for key, child := range val {
			if sub, ok := props[key].(map[string]any); ok {
				walk(root, sub, child, found, depth+1)
			} else if extra != nil {
				walk(root, extra, child, found, depth+1)
			}
		}
	case []any:
		if items, ok := node["items"].(map[string]any); ok {
			for _, child := range val {
				walk(root, items, child, found, depth+1)
			}
		}
	}
}

// logRefs answers each blob the records of every log entry name, with
// the entry that names it first.
func (d *described) logRefs(ctx context.Context, b Backend) (map[string]string, error) {
	out := map[string]string{}
	if len(d.records) == 0 {
		return out, nil
	}
	objs, err := b.List(ctx, "logs/", "")
	if err != nil {
		return nil, err
	}
	for _, o := range objs {
		if !strings.Contains(o.Key, "/entries/") {
			continue
		}
		data, _, err := b.Get(ctx, o.Key)
		if err != nil {
			return nil, err
		}
		var e struct {
			Changes []struct {
				Kind    string          `json:"kind"`
				Version uint32          `json:"version"`
				After   json.RawMessage `json:"after"`
			} `json:"changes"`
		}
		if json.Unmarshal(data, &e) != nil {
			continue // verify names an entry that is not JSON; gc keeps going
		}
		for _, c := range e.Changes {
			for _, h := range d.recordRefs(c.Kind, c.Version, c.After) {
				if _, ok := out[h]; !ok {
					out[h] = o.Key
				}
			}
		}
	}
	return out, nil
}
