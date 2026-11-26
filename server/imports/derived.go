package imports

// The server derives these documents (docs/architecture/storage.md,
// "The derived documents are in the record"; docs/FORMATS.md,
// "Derivations").
//
//	network, boundary  cap-network's import of the GTFS zip (source.feed)
//	streets            cap-streets' import of the street layer (source.streets)
//	walks              cap-streets' walk table over the streets and the network
//
// The derivation record is committed last, so it is the commit point. A
// crash before it leaves the documents' pointers ahead, and the next boot
// derives again and commits the same bytes.

import (
	"bytes"
	"encoding/json"
	"fmt"
	"log"
	"slices"

	"rustwasm/server/store"
)

const DerivationKind = "derivation"

const (
	DerivedNetworkPointer = "derived.network"
	DerivedStreetsPointer = "derived.streets"
	DerivedWalksPointer   = "derived.walks"
)

// Derivation is a host-owned kind (server/schemas/derivation.v1.schema.json).
type Derivation struct {
	Kind          string   `json:"kind"`
	SchemaVersion int      `json:"schema_version"`
	Inputs        []string `json:"inputs"`
	// Module is "<role>@sha256:<hex>".
	Module    string            `json:"module"`
	Documents []DerivedDocument `json:"documents"`
	Answer    json.RawMessage   `json:"answer"`
}

type DerivedDocument struct {
	Tag  string `json:"tag"`
	Hash string `json:"hash"`
}

type derivedDoc struct {
	tag       string
	storeKind string
	version   uint32
	body      []byte
	hash      string
}

// A stored document in another version is derived again.
var derivedKinds = map[string]struct {
	storeKind string
	version   uint32
}{
	"network":  {"network_document", NetworkVersion},
	"boundary": {"boundary", 1},
	"streets":  {"streets", 1},
	"walks":    {"walks", 1},
}

func isDerivedPointer(name string) bool {
	if _, ok := derivedKinds[name]; ok {
		return true
	}
	return name == DerivedNetworkPointer || name == DerivedStreetsPointer || name == DerivedWalksPointer
}

func StoredDerivation(st *store.Store, pointer string, inputs []string, tags ...string) (json.RawMessage, map[string][]byte, bool) {
	head, found, err := st.Head(pointer)
	if err != nil || !found {
		return nil, nil, false
	}
	raw, _, err := st.GetBlob(head.Hash)
	if err != nil {
		log.Printf("%s: the derivation %s does not read (%v): deriving again", pointer, head.Hash, err)
		return nil, nil, false
	}
	var d Derivation
	if err := json.Unmarshal(raw, &d); err != nil || d.Kind != DerivationKind || d.SchemaVersion != 1 {
		log.Printf("%s: %s is not a derivation v1: deriving again", pointer, head.Hash)
		return nil, nil, false
	}
	if !slices.Equal(d.Inputs, inputs) {
		return nil, nil, false
	}
	docs := map[string][]byte{}
	for _, tag := range tags {
		want := derivedKinds[tag]
		i := slices.IndexFunc(d.Documents, func(doc DerivedDocument) bool { return doc.Tag == tag })
		if i < 0 {
			return nil, nil, false
		}
		body, env, err := st.GetBlob(d.Documents[i].Hash)
		if err != nil {
			log.Printf("%s: its %s %s does not read (%v): deriving again", pointer, tag, d.Documents[i].Hash, err)
			return nil, nil, false
		}
		if env.Kind != want.storeKind || env.SchemaVersion != want.version {
			return nil, nil, false // a release that writes another version derives its own
		}
		docs[tag] = body
	}
	return d.Answer, docs, true
}

// A failed commit is logged, never fatal: the documents stand in memory,
// and the next derivation tries again.
func CommitDerivation(st *store.Store, pointer string, inputs []string, module string, answer json.RawMessage, docs ...derivedDoc) {
	d := Derivation{Kind: DerivationKind, SchemaVersion: 1, Inputs: inputs, Module: module, Documents: []DerivedDocument{}, Answer: answer}
	for _, doc := range docs {
		env := store.Envelope{Kind: doc.storeKind, SchemaVersion: doc.version, ProducedBy: module}
		if err := st.CommitHead(doc.tag, doc.body, env, doc.hash, "server"); err != nil {
			log.Printf("%s: commit its %s: %v", pointer, doc.tag, err)
			return
		}
		d.Documents = append(d.Documents, DerivedDocument{Tag: doc.tag, Hash: doc.hash})
	}
	body, err := jsonBytes(d)
	if err != nil {
		log.Printf("%s: %v", pointer, err)
		return
	}
	env := store.Envelope{Kind: DerivationKind, SchemaVersion: 1, ProducedBy: "server"}
	if err := st.CommitHead(pointer, body, env, "", "server"); err != nil {
		log.Printf("%s: commit the derivation: %v", pointer, err)
	}
}

func answerWith(answer json.RawMessage, fields map[string][]byte) (json.RawMessage, error) {
	var m map[string]json.RawMessage
	if err := json.Unmarshal(answer, &m); err != nil {
		return nil, fmt.Errorf("the stored answer: %w", err)
	}
	for k, v := range fields {
		m[k] = v
	}
	return jsonBytes(m)
}

func answerWithout(answer json.RawMessage, keys ...string) (json.RawMessage, error) {
	var m map[string]json.RawMessage
	if err := json.Unmarshal(answer, &m); err != nil {
		return nil, fmt.Errorf("the module's answer: %w", err)
	}
	for _, k := range keys {
		delete(m, k)
	}
	return jsonBytes(m)
}

// jsonBytes writes no HTML escapes and no trailing newline, so one value
// has one set of bytes.
func jsonBytes(v any) ([]byte, error) {
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	if err := enc.Encode(v); err != nil {
		return nil, err
	}
	return bytes.TrimSuffix(buf.Bytes(), []byte("\n")), nil
}
