package main

// The kind table (docs/FORMATS.md, "The kind table"; D-202): one table
// for each build, beside the server's modules (web/pkg/kinds.json,
// -kinds). The receive path reads it. A generic pointer write
// (POST /pointers/<name>) is governed by the entry of its kind:
//
//   - a kind with no entry is refused;
//   - a kind with its own route, or that only the server writes, is
//     refused, and the answer names its route;
//   - a kind written by pointer heads only its pointers, and is checked
//     by its module's validate op, which takes the document under the
//     kind's tag and answers the canonical bytes the server stores.
//
// /blobs never serves a kind whose read class is restricted. The host
// reads the table and decides nothing: each check is the module's.

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"strings"
)

// kindTable is web/pkg/kinds.json, as the host reads it.
type kindTable struct {
	Kinds []kindEntry `json:"kinds"`
	// raw are the kinds as the table states them, for the modules that
	// read the table's other fields (cap-log's existence check).
	raw json.RawMessage
}

type kindEntry struct {
	StoreKind string `json:"store_kind"`
	Tag       string `json:"tag"`
	Versions  []struct {
		Version uint32 `json:"version"`
		Schema  string `json:"schema"`
	} `json:"versions"`
	Write    string   `json:"write"` // pointer, route, server
	Route    string   `json:"route"`
	Pointers []string `json:"pointers"`
	Log      string   `json:"log,omitempty"`
	// Records are the log's record kinds of the kind's collection.
	Records []struct {
		Kind       string `json:"kind"`
		Collection string `json:"collection"`
	} `json:"records,omitempty"`
	// Receive is the capability whose receive op writes the kind's log.
	Receive string `json:"receive,omitempty"`
	// Intent false: a page never writes the kind's log by intent
	// (POST /logs/<log>); only the kind's routes write it.
	Intent *bool `json:"intent,omitempty"`
	// Post is the kind's own write route, answered by that receive op.
	Post *kindPost `json:"post,omitempty"`
	// Load is the kernel's load of the kind: a module registers a
	// document of it on this bulk op.
	Load *struct {
		Op   string `json:"op"`
		Code uint32 `json:"code"`
	} `json:"load,omitempty"`
	Module   string `json:"module,omitempty"`
	Validate string `json:"validate,omitempty"`
	Merge    string `json:"merge"`
	Read     string `json:"read"` // open, restricted, audit
}

// kindPost is a kind's own write route: the path, the op its body is
// the arguments of, the answer's field that names the document after
// the write, and the body's fields the answer repeats.
type kindPost struct {
	Path   string   `json:"path"`
	Op     string   `json:"op"`
	Answer string   `json:"answer"`
	Echo   []string `json:"echo,omitempty"`
}

// defaultKindsPath is where `just caps` writes the kind table.
const defaultKindsPath = "../web/pkg/kinds.json"

func loadKindTable(path string) (*kindTable, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("the kind table: %w (run `just caps`)", err)
	}
	var t kindTable
	if err := json.Unmarshal(raw, &t); err != nil {
		return nil, fmt.Errorf("the kind table %s: %w", path, err)
	}
	var kinds struct {
		Kinds json.RawMessage `json:"kinds"`
	}
	json.Unmarshal(raw, &kinds)
	t.raw = kinds.Kinds
	return &t, nil
}

// kindTable is the server's table, read on first use when main gave it
// none (tests, a server with no store).
func (s *server) kindTable() (*kindTable, error) {
	s.kindsOnce.Do(func() {
		if s.kinds != nil {
			return
		}
		path := s.kindsPath
		if path == "" {
			path = defaultKindsPath
		}
		s.kinds, s.kindsErr = loadKindTable(path)
	})
	return s.kinds, s.kindsErr
}

// byStoreKind is the entry of a store kind, or nil.
func (t *kindTable) byStoreKind(kind string) *kindEntry {
	for i := range t.Kinds {
		if t.Kinds[i].StoreKind == kind {
			return &t.Kinds[i]
		}
	}
	return nil
}

// heading is the entry whose pointers name `pointer`, or nil.
func (t *kindTable) heading(pointer string) *kindEntry {
	for i := range t.Kinds {
		if t.Kinds[i].heads(pointer) {
			return &t.Kinds[i]
		}
	}
	return nil
}

// heads says whether the kind heads `pointer`: a name in its pointers,
// or a prefix ending in `*`.
func (e *kindEntry) heads(pointer string) bool {
	for _, p := range e.Pointers {
		if prefix, ok := strings.CutSuffix(p, "*"); ok && strings.HasPrefix(pointer, prefix) || p == pointer {
			return true
		}
	}
	return false
}

// rule is the policy pointer whose write rule governs a write of the
// kind: its first pointer, else its store kind.
func (e *kindEntry) rule() string {
	for _, p := range e.Pointers {
		if !strings.HasSuffix(p, "*") {
			return p
		}
	}
	return e.StoreKind
}

// current is the newest version the table names for the kind (1 when it
// names none).
func (e *kindEntry) current() uint32 {
	v := uint32(1)
	for _, x := range e.Versions {
		if x.Version > v {
			v = x.Version
		}
	}
	return v
}

// restricted says whether /blobs must never serve a blob of `kind`. With
// no table, the policy is restricted: a missing table never opens it.
func (s *server) restricted(kind string) bool {
	t, err := s.kindTable()
	if err != nil {
		return kind == "policy"
	}
	e := t.byStoreKind(kind)
	return e == nil && kind == "policy" || e != nil && e.Read == "restricted"
}

// pointerRefusal is why the generic pointer write does not take `kind`
// under `name`, from the table ("" when it does), with the entry.
func (s *server) pointerRefusal(name, kind string) (*kindEntry, string, error) {
	t, err := s.kindTable()
	if err != nil {
		return nil, "", err
	}
	e := t.byStoreKind(kind)
	if e == nil {
		return nil, fmt.Sprintf("the store keeps no kind %q (the kind table names every kind it keeps)", kind), nil
	}
	if h := t.heading(name); h != nil && h != e {
		return nil, fmt.Sprintf("the %s pointer holds a %s document, not a %s one", name, h.StoreKind, kind), nil
	}
	if e.Write != "pointer" {
		return nil, fmt.Sprintf("a %s document is written through %s", kind, e.Route), nil
	}
	if !e.heads(name) {
		return nil, fmt.Sprintf("a %s document heads only the pointers %s", kind, strings.Join(e.Pointers, ", ")), nil
	}
	if e.Module == "" || e.Validate == "" {
		return nil, fmt.Sprintf("the kind table names no check for a %s write", kind), nil
	}
	return e, "", nil
}

// kindCheck is a module's answer to a write's check: the canonical
// document, or what refuses it, and what rides along.
type kindCheck struct {
	answer        json.RawMessage
	document      *string
	documentHash  string
	unknownFields []string
	module        string // "cap-<id>@sha256:<hex>"
}

// checkKind runs the entry's validate op over the document in a fresh
// instance of its module, closed after: a pointer write is rare.
func (s *server) checkKind(ctx context.Context, e *kindEntry, doc json.RawMessage) (kindCheck, error) {
	host, err := NewWasmHost(ctx, s.modulePath(e.Module))
	if err != nil {
		return kindCheck{}, fmt.Errorf("the %s module: %w", e.Module, err)
	}
	defer host.Close(ctx)
	answer, err := host.InvokeRaw(ctx, map[string]any{"op": e.Validate, e.Tag: doc})
	if err != nil {
		return kindCheck{}, err
	}
	var out struct {
		Document      *string  `json:"document"`
		DocumentHash  string   `json:"document_hash"`
		UnknownFields []string `json:"unknown_fields"`
	}
	if err := json.Unmarshal(answer, &out); err != nil {
		return kindCheck{}, err
	}
	return kindCheck{answer: answer, document: out.Document, documentHash: out.DocumentHash, unknownFields: out.UnknownFields, module: "cap-" + e.Module + "@" + host.Hash()}, nil
}
