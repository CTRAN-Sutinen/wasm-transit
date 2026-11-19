package main

// The derived documents in the record (D-199; docs/architecture/storage.md,
// "Derived documents"; docs/FORMATS.md, "Derivations"). In store mode a
// document the server derives from its inputs is stored when it is made:
//
//	network, boundary  cap-network's import of the GTFS zip (source.feed)
//	streets            cap-streets' import of the street layer (source.streets)
//	walks              cap-streets' walk table over the streets and the network
//
// Each document is the head of the pointer of its tag, with the module
// that made it in its envelope. A derivation record then names the
// inputs, the module, the documents, and the rest of the module's answer
// (an import's notes, its zone, and its first service date). It is
// committed last, on derived.<what>, so it is the commit point: a crash
// before it leaves the documents' pointers ahead, and the next boot
// derives again and commits the same bytes.
//
// A boot reads the record. When its inputs are the hashes of the inputs
// just read, and each document is stored in the version this build
// writes, the server serves the stored documents and runs no module. An
// input whose bytes changed is derived once more.

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"slices"
	"time"

	"rustwasm/server/store"
)

// DerivationKind is the derivation record's kind.
const DerivationKind = "derivation"

// The derivation records' pointers.
const (
	derivedNetworkPointer = "derived.network"
	derivedStreetsPointer = "derived.streets"
	derivedWalksPointer   = "derived.walks"
)

// Derivation names what a module made from which inputs. It is a
// host-owned kind (server/schemas/derivation.v1.schema.json).
type Derivation struct {
	Kind          string `json:"kind"`
	SchemaVersion int    `json:"schema_version"`
	// Inputs are the hashes the module read, in the order it read them.
	Inputs []string `json:"inputs"`
	// Module is the module that made the documents: "<role>@sha256:<hex>".
	Module string `json:"module"`
	// Documents are what it made, each the head of the pointer of its tag.
	Documents []DerivedDocument `json:"documents"`
	// Answer is the rest of the module's answer, as it answered it.
	Answer json.RawMessage `json:"answer"`
}

// DerivedDocument is one document a derivation made: its tag (the
// pointer it heads) and its hash.
type DerivedDocument struct {
	Tag  string `json:"tag"`
	Hash string `json:"hash"`
}

// derivedDoc is a document to store: its tag, its envelope's kind and
// version, its canonical bytes, and their hash.
type derivedDoc struct {
	tag       string
	storeKind string
	version   uint32
	body      []byte
	hash      string
}

// derivedKinds is each derived tag's envelope kind and the version this
// build writes. A stored document in another version is derived again.
var derivedKinds = map[string]struct {
	storeKind string
	version   uint32
}{
	"network":  {"network_document", networkVersion},
	"boundary": {"boundary", 1},
	"streets":  {"streets", 1},
	"walks":    {"walks", 1},
}

// isDerivedPointer says whether name is a pointer only the server's
// derivations write.
func isDerivedPointer(name string) bool {
	if _, ok := derivedKinds[name]; ok {
		return true
	}
	return name == derivedNetworkPointer || name == derivedStreetsPointer || name == derivedWalksPointer
}

// storedDerivation answers the stored derivation on pointer when it was
// made from inputs and holds the documents tags, each in the version
// this build writes: the module's answer and each document's bytes by
// tag. False when the server must derive again.
func storedDerivation(st *store.Store, pointer string, inputs []string, tags ...string) (json.RawMessage, map[string][]byte, bool) {
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

// commitDerivation stores each document as the head of its tag's
// pointer, then the derivation record on pointer. A failed commit is
// logged, never fatal: the documents stand in memory, and the next
// derivation tries again.
func commitDerivation(st *store.Store, pointer string, inputs []string, module string, answer json.RawMessage, docs ...derivedDoc) {
	d := Derivation{Kind: DerivationKind, SchemaVersion: 1, Inputs: inputs, Module: module, Documents: []DerivedDocument{}, Answer: answer}
	for _, doc := range docs {
		env := store.Envelope{Kind: doc.storeKind, SchemaVersion: doc.version, ProducedBy: module}
		if err := commitHead(st, doc.tag, doc.body, env, doc.hash); err != nil {
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
	if err := commitHead(st, pointer, body, env, ""); err != nil {
		log.Printf("%s: commit the derivation: %v", pointer, err)
	}
}

// commitHead makes body the head of pointer. Another process that
// committed the same bytes first is a success. Another head is retried
// over, a few times: a derivation of the same inputs makes the same
// bytes, so the last commit of the inputs just read stands.
func commitHead(st *store.Store, pointer string, body []byte, env store.Envelope, wantHash string) error {
	for attempt := 0; ; attempt++ {
		head, found, err := st.Head(pointer)
		if err != nil {
			return err
		}
		from := ""
		if found {
			from = head.Hash
		}
		_, _, err = st.CAS(pointer, from, body, env, wantHash, "server", time.Now().UnixMilli())
		var ce *store.ConflictError
		switch {
		case err == nil:
			return nil
		case !errors.As(err, &ce):
			return err
		case ce.Current == store.HashOf(body):
			return nil
		case attempt >= 2:
			return err
		}
	}
}

// answerWith answers the module's answer with fields set to the
// documents' bytes, as they are.
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

// answerWithout answers the module's answer with keys removed: the
// documents a derivation stores apart.
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

// jsonBytes marshals v with its strings as they are (no HTML escapes),
// and with no trailing newline: one value, one set of bytes.
func jsonBytes(v any) ([]byte, error) {
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	if err := enc.Encode(v); err != nil {
		return nil, err
	}
	return bytes.TrimSuffix(buf.Bytes(), []byte("\n")), nil
}
