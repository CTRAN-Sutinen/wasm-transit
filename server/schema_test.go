package main

// The host-owned document kinds have hand-written schemas, because no
// Rust type generates them (docs/FORMATS.md, "Builds"). Each schema is
// locked in server/schemas/ beside a frozen example. These tests hold
// the lock: the schema names exactly the Go type's fields, the example
// matches the schema, and a build the server really commits matches it
// too. `just caps` publishes both into the registry, marked host-owned.

import (
	"context"
	"encoding/json"
	"os"
	"reflect"
	"sort"
	"strings"
	"testing"

	"github.com/santhosh-tekuri/jsonschema/v6"

	"rustwasm/server/store"
)

const (
	buildSchemaPath      = "schemas/build.v1.schema.json"
	derivationSchemaPath = "schemas/derivation.v1.schema.json"
)

func readBuildSchema(t *testing.T) (map[string]any, *jsonschema.Schema) {
	return readHostSchema(t, buildSchemaPath)
}

// readHostSchema reads a host-owned kind's schema, as JSON and compiled.
func readHostSchema(t *testing.T, path string) (map[string]any, *jsonschema.Schema) {
	t.Helper()
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var doc map[string]any
	if err := json.Unmarshal(raw, &doc); err != nil {
		t.Fatal(err)
	}
	f, _ := os.Open(path)
	defer f.Close()
	inst, err := jsonschema.UnmarshalJSON(f)
	if err != nil {
		t.Fatal(err)
	}
	c := jsonschema.NewCompiler()
	url := "file:///" + path
	if err := c.AddResource(url, inst); err != nil {
		t.Fatal(err)
	}
	sch, err := c.Compile(url)
	if err != nil {
		t.Fatal(err)
	}
	return doc, sch
}

// jsonFields is a struct type's JSON field names, and which of them it
// always writes.
func jsonFields(t reflect.Type) (names []string, required []string) {
	for i := 0; i < t.NumField(); i++ {
		tag := t.Field(i).Tag.Get("json")
		name, opts, _ := strings.Cut(tag, ",")
		if name == "" || name == "-" {
			continue
		}
		names = append(names, name)
		if !strings.Contains(opts, "omitempty") {
			required = append(required, name)
		}
	}
	sort.Strings(names)
	sort.Strings(required)
	return names, required
}

// schemaFields is an object schema's property names and its required
// list.
func schemaFields(t *testing.T, obj map[string]any) (names []string, required []string) {
	t.Helper()
	props, _ := obj["properties"].(map[string]any)
	for name := range props {
		names = append(names, name)
	}
	reqs, _ := obj["required"].([]any)
	for _, r := range reqs {
		required = append(required, r.(string))
	}
	if obj["additionalProperties"] != false {
		t.Errorf("a host-owned object is closed: %v", obj["title"])
	}
	sort.Strings(names)
	sort.Strings(required)
	return names, required
}

func TestTheBuildSchemaNamesExactlyTheGoTypesFields(t *testing.T) {
	doc, _ := readBuildSchema(t)
	defs := doc["$defs"].(map[string]any)
	for _, c := range []struct {
		goType reflect.Type
		schema map[string]any
	}{
		{reflect.TypeOf(store.BuildDoc{}), doc},
		{reflect.TypeOf(store.BuildFile{}), defs["BuildFile"].(map[string]any)},
		{reflect.TypeOf(store.BuildModule{}), defs["BuildModule"].(map[string]any)},
	} {
		goNames, goRequired := jsonFields(c.goType)
		names, required := schemaFields(t, c.schema)
		if !reflect.DeepEqual(goNames, names) || !reflect.DeepEqual(goRequired, required) {
			t.Errorf("%s: the Go type has fields %v (always written: %v), and the schema has %v (required: %v). Change the schema with the type, and bump schema_version for anything but an added optional field.",
				c.goType.Name(), goNames, goRequired, names, required)
		}
	}
}

func TestTheBuildExampleMatchesItsSchema(t *testing.T) {
	_, sch := readBuildSchema(t)
	raw, err := os.ReadFile("schemas/build.v1.example.json")
	if err != nil {
		t.Fatal(err)
	}
	validateBytes(t, sch, "the frozen build example", raw)
	if _, err := store.ParseBuild(raw); err != nil {
		t.Fatalf("the frozen build example does not parse as a build: %v", err)
	}
}

// What the server emits: the build it commits from a dist, with its
// hosted module retained, and a UI's bundle with none.
func TestACommittedBuildMatchesItsSchema(t *testing.T) {
	_, sch := readBuildSchema(t)
	dist := t.TempDir()
	writeDist(t, dist, releaseA)
	s, base := newBuildServer(t, store.NewMemory(), dist)
	data, _, err := s.store.GetBlob(manifestBuild(t, base))
	if err != nil {
		t.Fatal(err)
	}
	validateBytes(t, sch, "a committed build", data)

	bundle, err := json.Marshal(store.BuildDoc{Kind: store.BuildKind, SchemaVersion: 1, Files: []store.BuildFile{}}.Sorted())
	if err != nil {
		t.Fatal(err)
	}
	validateBytes(t, sch, "a UI's bundle with no server modules", bundle)
}

func TestTheDerivationSchemaNamesExactlyTheGoTypesFields(t *testing.T) {
	doc, _ := readHostSchema(t, derivationSchemaPath)
	defs := doc["$defs"].(map[string]any)
	for _, c := range []struct {
		goType reflect.Type
		schema map[string]any
	}{
		{reflect.TypeOf(Derivation{}), doc},
		{reflect.TypeOf(DerivedDocument{}), defs["DerivedDocument"].(map[string]any)},
	} {
		goNames, goRequired := jsonFields(c.goType)
		names, required := schemaFields(t, c.schema)
		if !reflect.DeepEqual(goNames, names) || !reflect.DeepEqual(goRequired, required) {
			t.Errorf("%s: the Go type has fields %v (always written: %v), and the schema has %v (required: %v). Change the schema with the type, and bump schema_version for anything but an added optional field.",
				c.goType.Name(), goNames, goRequired, names, required)
		}
	}
}

func TestTheDerivationExampleMatchesItsSchema(t *testing.T) {
	_, sch := readHostSchema(t, derivationSchemaPath)
	raw, err := os.ReadFile("schemas/derivation.v1.example.json")
	if err != nil {
		t.Fatal(err)
	}
	validateBytes(t, sch, "the frozen derivation example", raw)
}

// What the server emits: the derivations a store-mode boot commits for
// its feed, its street layer, and the walk table over them.
func TestCommittedDerivationsMatchTheirSchema(t *testing.T) {
	_, sch := readHostSchema(t, derivationSchemaPath)
	s := derivedServer(t, store.NewMemory(), writeLayer(t))
	if _, err := s.buildManifest(); err != nil {
		t.Fatal(err)
	}
	for _, pointer := range []string{derivedNetworkPointer, derivedStreetsPointer, derivedWalksPointer} {
		head, found, err := s.store.Head(pointer)
		if err != nil || !found {
			t.Fatalf("%s: not committed (%v)", pointer, err)
		}
		data, _, err := s.store.GetBlob(head.Hash)
		if err != nil {
			t.Fatal(err)
		}
		validateBytes(t, sch, pointer, data)
	}
}

const layoutSchemaPath = "schemas/store_layout.v1.schema.json"

func TestTheLayoutSchemaNamesExactlyTheGoTypesFields(t *testing.T) {
	doc, _ := readHostSchema(t, layoutSchemaPath)
	goNames, goRequired := jsonFields(reflect.TypeOf(store.Layout{}))
	names, required := schemaFields(t, doc)
	if !reflect.DeepEqual(goNames, names) || !reflect.DeepEqual(goRequired, required) {
		t.Errorf("Layout: the Go type has %v (%v), the schema %v (%v)", goNames, goRequired, names, required)
	}
}

// The frozen example, and the layout a store writes, match the schema.
func TestTheLayoutMatchesItsSchema(t *testing.T) {
	_, sch := readHostSchema(t, layoutSchemaPath)
	raw, err := os.ReadFile("schemas/store_layout.v1.example.json")
	if err != nil {
		t.Fatal(err)
	}
	validateBytes(t, sch, "the frozen layout example", raw)
	bucket := store.NewMemory()
	newStoreServer(t, bucket)
	written, _, err := bucket.Get(context.Background(), store.LayoutKey)
	if err != nil {
		t.Fatalf("a store-mode boot writes the layout: %v", err)
	}
	validateBytes(t, sch, "the layout a store writes", written)
}
