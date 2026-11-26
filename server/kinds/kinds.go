// Package kinds reads the kind table (docs/FORMATS.md, "The kind table";
// D-202): how each kind the store keeps is written, checked, and read.
// The host reads the table and decides nothing. `node tools/caps-run.mjs
// kind <tag>` prints one kind's path through the code.
package kinds

import (
	"encoding/json"
	"fmt"
	"os"
	"strings"
)

type Table struct {
	Kinds []Entry `json:"kinds"`
	// Raw keeps the table's other fields for cap-log's existence check.
	Raw json.RawMessage `json:"-"`
}

type Entry struct {
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
	Intent     *bool      `json:"intent,omitempty"`
	Post       *Post      `json:"post,omitempty"`
	WorkOrder  *WorkOrder `json:"work_order,omitempty"`
	Pick       *Pick      `json:"pick,omitempty"`
	FeedEvents string     `json:"feed_events,omitempty"`
	Load       *struct {
		Op   string `json:"op"`
		Code uint32 `json:"code"`
	} `json:"load,omitempty"`
	Module   string `json:"module,omitempty"`
	Validate string `json:"validate,omitempty"`
	Merge    string `json:"merge"`
	Read     string `json:"read"` // open, restricted, audit
}

type WorkOrder struct {
	Method string `json:"method"`
	Path   string `json:"path"`
	Call   string `json:"call"`
	Record string `json:"record"`
}

type Pick struct {
	Seq    string `json:"seq"`
	Export string `json:"export"`
}

type Post struct {
	Path   string   `json:"path"`
	Op     string   `json:"op"`
	Answer string   `json:"answer"`
	Echo   []string `json:"echo,omitempty"`
}

// DefaultPath is the table `just caps` writes, from the server's directory.
const DefaultPath = "../web/pkg/kinds.json"

func Load(path string) (*Table, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("the kind table: %w (run `just caps`)", err)
	}
	var t Table
	if err := json.Unmarshal(raw, &t); err != nil {
		return nil, fmt.Errorf("the kind table %s: %w", path, err)
	}
	var kinds struct {
		Kinds json.RawMessage `json:"kinds"`
	}
	json.Unmarshal(raw, &kinds)
	t.Raw = kinds.Kinds
	return &t, nil
}

// Where is the first kind the table names that has the field, or nil.
func (t *Table) Where(has func(*Entry) bool) *Entry {
	for i := range t.Kinds {
		if has(&t.Kinds[i]) {
			return &t.Kinds[i]
		}
	}
	return nil
}

func (t *Table) WorkOrderKind() *Entry {
	return t.Where(func(e *Entry) bool { return e.WorkOrder != nil })
}

// PickKind is the kind a pick reads (Pick).
func (t *Table) PickKind() *Entry {
	return t.Where(func(e *Entry) bool { return e.Pick != nil && e.Load != nil })
}

func (t *Table) ByStoreKind(kind string) *Entry {
	return t.Where(func(e *Entry) bool { return e.StoreKind == kind })
}

// Heading is the kind whose pointers include the pointer.
func (t *Table) Heading(pointer string) *Entry {
	return t.Where(func(e *Entry) bool { return e.Heads(pointer) })
}

// HashField names the kind's document in an op's arguments and answers.
func (e *Entry) HashField() string {
	if e.Post != nil {
		return e.Post.Answer
	}
	return e.StoreKind + "_hash"
}

// A pointer pattern ending in `*` is a prefix.
func (e *Entry) Heads(pointer string) bool {
	for _, p := range e.Pointers {
		if prefix, ok := strings.CutSuffix(p, "*"); ok && strings.HasPrefix(pointer, prefix) || p == pointer {
			return true
		}
	}
	return false
}

// Rule is the pointer that the policy's write rule names for the kind.
func (e *Entry) Rule() string {
	for _, p := range e.Pointers {
		if !strings.HasSuffix(p, "*") {
			return p
		}
	}
	return e.StoreKind
}

func (e *Entry) Current() uint32 {
	v := uint32(1)
	for _, x := range e.Versions {
		if x.Version > v {
			v = x.Version
		}
	}
	return v
}

// PointerRefusal answers why a generic pointer write of the kind is
// refused, or the kind when it is not.
func (t *Table) PointerRefusal(name, kind string) (*Entry, string) {
	e := t.ByStoreKind(kind)
	if e == nil {
		return nil, fmt.Sprintf("the store keeps no kind %q (the kind table names every kind it keeps)", kind)
	}
	if h := t.Heading(name); h != nil && h != e {
		return nil, fmt.Sprintf("the %s pointer holds a %s document, not a %s one", name, h.StoreKind, kind)
	}
	if e.Write != "pointer" {
		return nil, fmt.Sprintf("a %s document is written through %s", kind, e.Route)
	}
	if !e.Heads(name) {
		return nil, fmt.Sprintf("a %s document heads only the pointers %s", kind, strings.Join(e.Pointers, ", "))
	}
	if e.Module == "" || e.Validate == "" {
		return nil, fmt.Sprintf("the kind table names no check for a %s write", kind)
	}
	return e, ""
}
