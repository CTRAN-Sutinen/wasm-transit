package main

// A reader outside the wasm (docs/FORMATS.md): the bucket's objects,
// the generated JSON Schemas the registry publishes
// (registry/documents.json), and docs/FORMATS.md are enough
// to read what the store holds. Nothing below the setup calls a module,
// links Rust, or uses the store engine — only object get and list: the
// pointer is its highest record object, the blob is the object its hash
// names, the blob's envelope metadata names the kind and schema version,
// and the schema says what the bytes are.

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"

	"github.com/santhosh-tekuri/jsonschema/v6"

	"rustwasm/server/store"
)

const registryDir = "../registry"

// documentIndex is registry/documents.json.
type documentIndex struct {
	Documents []struct {
		Kind          string  `json:"kind"`
		StoreKind     *string `json:"store_kind"`
		SchemaVersion uint32  `json:"schema_version"`
		Schema        string  `json:"schema"`
		Example       string  `json:"example"`
	} `json:"documents"`
}

func loadDocumentIndex(t *testing.T) documentIndex {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join(registryDir, "documents.json"))
	if err != nil {
		t.Fatalf("the registry's documents index (run `just caps`): %v", err)
	}
	var idx documentIndex
	if err := json.Unmarshal(raw, &idx); err != nil {
		t.Fatal(err)
	}
	return idx
}

func compileSchema(t *testing.T, path string) *jsonschema.Schema {
	t.Helper()
	f, err := os.Open(filepath.Join(registryDir, path))
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	doc, err := jsonschema.UnmarshalJSON(f)
	if err != nil {
		t.Fatalf("%s: %v", path, err)
	}
	c := jsonschema.NewCompiler()
	c.AssertFormat() // "date" is a promise: YYYY-MM-DD
	url := "file:///" + path
	if err := c.AddResource(url, doc); err != nil {
		t.Fatal(err)
	}
	sch, err := c.Compile(url)
	if err != nil {
		t.Fatalf("compile %s: %v", path, err)
	}
	return sch
}

func validateBytes(t *testing.T, sch *jsonschema.Schema, what string, data []byte) {
	t.Helper()
	inst, err := jsonschema.UnmarshalJSON(bytes.NewReader(data))
	if err != nil {
		t.Fatalf("%s: not JSON: %v", what, err)
	}
	if err := sch.Validate(inst); err != nil {
		t.Fatalf("%s does not match its schema: %v", what, err)
	}
}

// Every worked example the registry publishes is a document of its own
// schema: the schemas describe the bytes the modules really make.
func TestRegistryExamplesMatchTheirSchemas(t *testing.T) {
	idx := loadDocumentIndex(t)
	if len(idx.Documents) == 0 {
		t.Fatal("the registry describes no document kinds")
	}
	for _, d := range idx.Documents {
		if d.Example == "" {
			continue
		}
		example, err := os.ReadFile(filepath.Join(registryDir, d.Example))
		if err != nil {
			t.Fatal(err)
		}
		validateBytes(t, compileSchema(t, d.Schema), d.Example, example)
	}
}

// storeReader reads a bucket the way FORMATS.md says, with nothing but
// object get, list, and JSON.
type storeReader struct{ b store.Backend }

type logLine struct {
	Seq           int64  `json:"seq"`
	To            string `json:"to"`
	Kind          string `json:"kind"`
	SchemaVersion uint32 `json:"schema_version"`
}

func (r storeReader) head(t *testing.T, pointer string) logLine {
	t.Helper()
	ctx := context.Background()
	records, err := r.b.List(ctx, "pointers/"+pointer+"/", "")
	if err != nil || len(records) == 0 {
		t.Fatalf("pointer %s: %d records, %v", pointer, len(records), err)
	}
	// Keys are zero-padded: the last listed is the head.
	raw, _, err := r.b.Get(ctx, records[len(records)-1].Key)
	if err != nil {
		t.Fatal(err)
	}
	var last logLine
	if err := json.Unmarshal(raw, &last); err != nil {
		t.Fatal(err)
	}
	if last.Seq != int64(len(records)) {
		t.Fatalf("pointer %s: head seq %d over %d records", pointer, last.Seq, len(records))
	}
	return last
}

func (r storeReader) blob(t *testing.T, hash string) ([]byte, struct {
	Kind          string `json:"kind"`
	SchemaVersion uint32 `json:"schema_version"`
}) {
	t.Helper()
	hexPart := strings.TrimPrefix(hash, "sha256:")
	data, meta, err := r.b.Get(context.Background(), "blobs/"+hexPart)
	if err != nil {
		t.Fatal(err)
	}
	if sha(data) != hash {
		t.Fatalf("blob %s: the bytes are not the ones its name hashes", hash)
	}
	var env struct {
		Kind          string `json:"kind"`
		SchemaVersion uint32 `json:"schema_version"`
	}
	if err := json.Unmarshal([]byte(meta["envelope"]), &env); err != nil {
		t.Fatal(err)
	}
	return data, env
}

// schemaFor finds the schema for a stored blob from its envelope alone.
func schemaFor(t *testing.T, idx documentIndex, kind string, version uint32) *jsonschema.Schema {
	t.Helper()
	for _, d := range idx.Documents {
		if d.StoreKind != nil && *d.StoreKind == kind && d.SchemaVersion == version {
			return compileSchema(t, d.Schema)
		}
	}
	t.Fatalf("the registry has no schema for store kind %q v%d", kind, version)
	return nil
}

// The headline: a store written by the server (modules and all) is
// read back by plain Go — the published plan chain, every pick's
// network, and the edit document, each validated against the schema
// its envelope names.
func TestStoreReadsWithoutTheModules(t *testing.T) {
	bucket := store.NewMemory()
	s, base := newPlansServer(t, bucket)
	for _, p := range []struct{ id, from string }{{"winter-2026", "2026-12-01"}, {"fall-2026", "2026-09-07"}} {
		n := fixtureNetwork(t, p.id, p.id, p.from)
		n["name"] = p.id
		if code, out := publishPick(t, base, n); code != 200 {
			t.Fatalf("publish %s: %d %v", p.id, code, out)
		}
	}
	if code, _, body := httpPost(t, base+"/edits", editDoc("e1", "draft", 1, 100)); code != 200 {
		t.Fatalf("edits: %d %s", code, body)
	}
	s.store.Close()

	// From here on: objects, JSON, and the published schemas.
	idx := loadDocumentIndex(t)
	r := storeReader{bucket}

	head := r.head(t, "plans")
	data, env := r.blob(t, head.To)
	if env.Kind != "plans" || head.Kind != "plans" {
		t.Fatalf("plans envelope: %+v", env)
	}
	validateBytes(t, schemaFor(t, idx, env.Kind, env.SchemaVersion), "the published plan set", data)
	var set publishedSet
	if err := json.Unmarshal(data, &set); err != nil {
		t.Fatal(err)
	}
	// The plan chain, read straight off the document.
	var chain []string
	for _, p := range set.Plans {
		chain = append(chain, p.EffectiveFrom+" "+p.ID)
		netData, netEnv := r.blob(t, p.NetworkHash)
		if netEnv.Kind != "network_document" {
			t.Fatalf("pick %s names a %s", p.ID, netEnv.Kind)
		}
		validateBytes(t, schemaFor(t, idx, netEnv.Kind, netEnv.SchemaVersion), "pick "+p.ID+"'s network", netData)
		if r.head(t, "plan."+p.ID).To != p.NetworkHash {
			t.Fatalf("pick %s: the set and its pointer disagree", p.ID)
		}
	}
	if strings.Join(chain, ",") != "2026-09-07 fall-2026,2026-12-01 winter-2026" {
		t.Fatalf("the chain in date order: %v", chain)
	}

	// The edit document is the disruptions log: each entry says its kind
	// and version inside it, and validates against that schema.
	entries, err := bucket.List(context.Background(), "logs/disruptions/entries/", "")
	if err != nil || len(entries) == 0 {
		t.Fatalf("the disruptions log: %d entries, %v", len(entries), err)
	}
	for _, o := range entries {
		raw, _, err := bucket.Get(context.Background(), o.Key)
		if err != nil {
			t.Fatal(err)
		}
		var tag struct {
			Kind          string `json:"kind"`
			SchemaVersion uint32 `json:"schema_version"`
		}
		if err := json.Unmarshal(raw, &tag); err != nil || tag.Kind != "log_entry" {
			t.Fatalf("%s: %s %v", o.Key, raw, err)
		}
		validateBytes(t, schemaForKind(t, idx, tag.Kind, tag.SchemaVersion), o.Key, raw)
	}
}

// schemaForKind is the published schema of a document by the kind and
// version it says inside it.
func schemaForKind(t *testing.T, idx documentIndex, kind string, version uint32) *jsonschema.Schema {
	t.Helper()
	for _, d := range idx.Documents {
		if d.Kind == kind && d.SchemaVersion == version {
			return compileSchema(t, d.Schema)
		}
	}
	t.Fatalf("the registry has no schema for kind %q v%d", kind, version)
	return nil
}

// servedSchemasDir is the /schemas/ every page build carries, written
// by `just caps` (gen-manifest) into web/public/schemas/.
const servedSchemasDir = "../web/public/schemas"

// withSchemas puts the build's /schemas/ into a -dist.
func withSchemas(t *testing.T, dist string) {
	t.Helper()
	if _, err := os.Stat(filepath.Join(servedSchemasDir, "documents.json")); err != nil {
		t.Fatalf("the served schemas (run `just caps`): %v", err)
	}
	if err := copyTree(servedSchemasDir, filepath.Join(dist, "schemas")); err != nil {
		t.Fatal(err)
	}
}

// schemaOf finds a document's schema the way a reader with nothing but
// a deployment does: the blob, the tag and version inside it, then
// /schemas/documents.json, then the schema file it names, whose $id
// names that tag and version. get answers a path under the deployment's
// root. It returns the schema's path under /schemas/. That the bytes
// match the schema is the Rust gates' to check (docs/FORMATS.md, "The
// schema lock"); this checks that a reader can get there.
func schemaOf(t *testing.T, get func(path string) []byte, hash, what string) string {
	t.Helper()
	var head struct {
		Kind          string `json:"kind"`
		SchemaVersion uint32 `json:"schema_version"`
	}
	if err := json.Unmarshal(get("blobs/"+hash), &head); err != nil || head.Kind == "" || head.SchemaVersion == 0 {
		t.Fatalf("%s: the document names no kind and version inside it: %+v (%v)", what, head, err)
	}
	var index struct {
		Documents []struct {
			Tag           string `json:"tag"`
			SchemaVersion uint32 `json:"schema_version"`
			Schema        string `json:"schema"`
		} `json:"documents"`
	}
	if err := json.Unmarshal(get("schemas/documents.json"), &index); err != nil {
		t.Fatalf("/schemas/documents.json: %v", err)
	}
	for _, d := range index.Documents {
		if d.Tag != head.Kind || d.SchemaVersion != head.SchemaVersion {
			continue
		}
		var schema struct {
			ID string `json:"$id"`
		}
		if err := json.Unmarshal(get("schemas/"+d.Schema), &schema); err != nil {
			t.Fatalf("%s: /schemas/%s is not JSON: %v", what, d.Schema, err)
		}
		if want := "urn:transit-document:" + head.Kind + ":v" + strconv.FormatUint(uint64(head.SchemaVersion), 10); schema.ID != want {
			t.Fatalf("%s: /schemas/%s has $id %q, want %q", what, d.Schema, schema.ID, want)
		}
		return d.Schema
	}
	t.Fatalf("%s: /schemas/ has no schema for %s v%d", what, head.Kind, head.SchemaVersion)
	return ""
}

func linkTo(schema string) string {
	return `</schemas/` + schema + `>; rel="describedby"; type="application/schema+json"`
}

// A static export holds its schemas beside its blobs, and every
// published document finds its schema there.
func TestAnExportReachesEachDocumentsSchema(t *testing.T) {
	s := exportServer(t)
	withSchemas(t, s.dist)
	bundle := filepath.Join(t.TempDir(), "bundle")
	if err := s.export(bundle, ""); err != nil {
		t.Fatalf("export: %v", err)
	}
	raw, err := os.ReadFile(filepath.Join(bundle, "manifest"))
	if err != nil {
		t.Fatal(err)
	}
	var m Manifest
	if err := json.Unmarshal(raw, &m); err != nil {
		t.Fatal(err)
	}
	get := func(path string) []byte {
		data, err := os.ReadFile(filepath.Join(bundle, filepath.FromSlash(path)))
		if err != nil {
			t.Fatalf("the bundle's %s: %v", path, err)
		}
		return data
	}
	for what, hash := range map[string]string{"network": m.NetworkHash, "boundary": m.BoundaryHash, "streets": m.StreetsHash, "walks": m.WalksHash} {
		schemaOf(t, get, hash, "the exported "+what)
	}
}

// The same from a running server: /schemas/ is served, open, from the
// pages, and a blob's response links the schema a reader finds.
func TestAServerLinksEachDocumentsSchema(t *testing.T) {
	s := exportServer(t)
	withSchemas(t, s.dist)
	base := httptest.NewServer(s.routes()).URL
	get := func(path string) []byte {
		code, _, body := httpGet(t, base+"/"+path)
		if code != 200 {
			t.Fatalf("GET /%s: %d", path, code)
		}
		return []byte(body)
	}
	var m Manifest
	if err := json.Unmarshal(get("manifest"), &m); err != nil {
		t.Fatal(err)
	}
	for what, hash := range map[string]string{"network": m.NetworkHash, "boundary": m.BoundaryHash, "streets": m.StreetsHash, "walks": m.WalksHash} {
		schema := schemaOf(t, get, hash, "the served "+what)
		if _, hdr, _ := httpGet(t, base+"/blobs/"+hash); hdr.Get("Link") != linkTo(schema) {
			t.Fatalf("/blobs/%s (%s) links %q, want %q", hash, what, hdr.Get("Link"), linkTo(schema))
		}
	}
}

// A stored blob links its schema by its envelope, in store mode, where
// the pages (and so /schemas/) come from the pinned build.
func TestAStoredBlobLinksItsSchema(t *testing.T) {
	dist := t.TempDir()
	writeDist(t, dist, releaseA)
	withSchemas(t, dist)
	s, base := newBuildServer(t, store.NewMemory(), dist)
	// A stored blob: the edit document in its stored kind.
	hash, err := s.store.PutBlob([]byte(editDoc("e1", "submitted", 2, 100)), s.syncEnvelope(), "")
	if err != nil {
		t.Fatal(err)
	}
	get := func(path string) []byte {
		code, _, body := httpGet(t, base+"/"+path)
		if code != 200 {
			t.Fatalf("GET /%s: %d", path, code)
		}
		return []byte(body)
	}
	schema := schemaOf(t, get, hash, "the stored edit document")
	if _, hdr, _ := httpGet(t, base+"/blobs/"+hash); hdr.Get("Link") != linkTo(schema) {
		t.Fatalf("the stored edit document links %q, want %q", hdr.Get("Link"), linkTo(schema))
	}
}
