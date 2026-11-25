package main

// The kind table (docs/FORMATS.md, "The kind table"; D-202) governs a
// generic pointer write (POST /pointers/<name>). The host reads the table
// and decides nothing: each check is the module's.

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"strings"
)

type kindTable struct {
	Kinds []kindEntry `json:"kinds"`
	// raw keeps the table's other fields for cap-log's existence check.
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
	Records  []struct {
		Kind       string `json:"kind"`
		Collection string `json:"collection"`
	} `json:"records,omitempty"`
	Receive string `json:"receive,omitempty"`
	// A false Intent means only the kind's routes write the log, never
	// POST /logs/<log>.
	Intent *bool     `json:"intent,omitempty"`
	Post   *kindPost `json:"post,omitempty"`
	Load   *struct {
		Op   string `json:"op"`
		Code uint32 `json:"code"`
	} `json:"load,omitempty"`
	Module   string `json:"module,omitempty"`
	Validate string `json:"validate,omitempty"`
	Merge    string `json:"merge"`
	Read     string `json:"read"` // open, restricted, audit
}

type kindPost struct {
	Path   string   `json:"path"`
	Op     string   `json:"op"`
	Answer string   `json:"answer"`
	Echo   []string `json:"echo,omitempty"`
}

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

func (t *kindTable) byStoreKind(kind string) *kindEntry {
	for i := range t.Kinds {
		if t.Kinds[i].StoreKind == kind {
			return &t.Kinds[i]
		}
	}
	return nil
}

func (t *kindTable) heading(pointer string) *kindEntry {
	for i := range t.Kinds {
		if t.Kinds[i].heads(pointer) {
			return &t.Kinds[i]
		}
	}
	return nil
}

// A pointer pattern ending in `*` is a prefix.
func (e *kindEntry) heads(pointer string) bool {
	for _, p := range e.Pointers {
		if prefix, ok := strings.CutSuffix(p, "*"); ok && strings.HasPrefix(pointer, prefix) || p == pointer {
			return true
		}
	}
	return false
}

func (e *kindEntry) rule() string {
	for _, p := range e.Pointers {
		if !strings.HasSuffix(p, "*") {
			return p
		}
	}
	return e.StoreKind
}

func (e *kindEntry) current() uint32 {
	v := uint32(1)
	for _, x := range e.Versions {
		if x.Version > v {
			v = x.Version
		}
	}
	return v
}

// With no table, the policy is restricted: a missing table never opens it.
func (s *server) restricted(kind string) bool {
	t, err := s.kindTable()
	if err != nil {
		return kind == "policy"
	}
	e := t.byStoreKind(kind)
	return e == nil && kind == "policy" || e != nil && e.Read == "restricted"
}

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

type kindCheck struct {
	answer        json.RawMessage
	document      *string
	documentHash  string
	unknownFields []string
	module        string
}

// A pointer write is rare, so the module instance is fresh and closed
// after.
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
