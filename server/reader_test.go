package main

// A reader outside the wasm (docs/FORMATS.md): the store directory,
// the generated JSON Schemas the registry publishes
// (registry/documents.json), and docs/FORMATS.md are enough
// to read what the store holds. Nothing below the setup calls a module
// or links Rust: the pointer is the log's last line, the blob is the
// file its hash names, the envelope sidecar names the kind and schema
// version, and the schema says what the bytes are.

import (
	"bufio"
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/santhosh-tekuri/jsonschema/v6"
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

// storeReader reads a store directory the way FORMATS.md says, with
// nothing but files and JSON.
type storeReader struct{ dir string }

type logLine struct {
	Seq           int64  `json:"seq"`
	To            string `json:"to"`
	Kind          string `json:"kind"`
	SchemaVersion uint32 `json:"schema_version"`
}

func (r storeReader) head(t *testing.T, pointer string) logLine {
	t.Helper()
	f, err := os.Open(filepath.Join(r.dir, "pointers", pointer+".log"))
	if err != nil {
		t.Fatalf("pointer %s: %v", pointer, err)
	}
	defer f.Close()
	var last logLine
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 1<<20), 1<<20)
	for sc.Scan() {
		if err := json.Unmarshal(sc.Bytes(), &last); err != nil {
			t.Fatal(err)
		}
	}
	return last
}

func (r storeReader) blob(t *testing.T, hash string) ([]byte, struct {
	Kind          string `json:"kind"`
	SchemaVersion uint32 `json:"schema_version"`
}) {
	t.Helper()
	hexPart := strings.TrimPrefix(hash, "sha256:")
	data, err := os.ReadFile(filepath.Join(r.dir, "blobs", hexPart))
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
	meta, err := os.ReadFile(filepath.Join(r.dir, "blobs", hexPart+".meta.json"))
	if err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(meta, &env); err != nil {
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
	dir := t.TempDir()
	s, base := newPlansServer(t, dir)
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

	// From here on: files, JSON, and the published schemas.
	idx := loadDocumentIndex(t)
	r := storeReader{dir}

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

	edits := r.head(t, "edits")
	editData, editEnv := r.blob(t, edits.To)
	validateBytes(t, schemaFor(t, idx, editEnv.Kind, editEnv.SchemaVersion), "the edit document", editData)
}
