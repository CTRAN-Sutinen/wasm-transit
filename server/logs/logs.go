// Package logs keeps the logs of records (D-190; docs/FORMATS.md, "The
// logs"; docs/architecture/server.md, "One path for a log"). The host
// orders and stores entries and checkpoints, and treats every entry,
// checkpoint, record, and document as opaque bytes. cap-log folds them,
// and a log's receiving capability writes each entry through its
// `receive` op: the op answers the entry and the documents it wrote, and
// the host decides nothing.
//
// One entry point each: Receive writes one entry, a log's fold answers
// its documents (Head, Current, Pins), and HandleLogs serves /logs/. A
// key that landed answers the entry that holds it, so a resend lands
// once. A write is a 409 only when an entry after the page's `read_seq`
// changed a record that the write changes.
//
// `node tools/caps-run.mjs kind <tag>` prints a kind's log, its routes,
// and the ops that write and fold it.
package logs

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"sort"
	"sync"

	"rustwasm/server/kinds"
	"rustwasm/server/store"
	"rustwasm/server/wasmhost"
)

// Deps is what the host gives the logs. Each is read when a log is first
// used, so a test sets the server's fields before.
type Deps struct {
	Store *store.Store
	Table func() (*kinds.Table, error)
	Long  func() *wasmhost.Held
	// MayWrite is the policy's write rule for a person's write.
	MayWrite func(r *http.Request, rule string) (bool, string)
	// WriteDate is today in the agency's zone, the date a reference rule
	// reads a dated state on.
	WriteDate func() string
	// Forward brings an older document to the current version (cap-migrate).
	Forward func(kind string, doc json.RawMessage) (json.RawMessage, bool, error)
}

// Logs are the logs that the kind table names with a receiving
// capability. With no store or no table, there are none.
type Logs struct {
	store     *store.Store
	table     func() (*kinds.Table, error)
	long      func() *wasmhost.Held
	mayWrite  func(r *http.Request, rule string) (bool, string)
	writeDate func() string
	forward   func(kind string, doc json.RawMessage) (json.RawMessage, bool, error)

	once sync.Once
	kept map[string]*Log
}

func New(d Deps) *Logs {
	return &Logs{store: d.Store, table: d.Table, long: d.Long, mayWrite: d.MayWrite, writeDate: d.WriteDate, forward: d.Forward}
}

func (l *Log) Name() string { return l.name }

// Rule is the write rule of the log's first kind, or "" for none.
func (l *Log) Rule() string {
	if len(l.kinds) == 0 {
		return ""
	}
	return l.kinds[0].Rule()
}

// docs is guarded by fold.mu.
type Log struct {
	name    string
	receive string
	kinds   []kinds.Entry
	fold    *fold
	docs    map[string]logDoc
	intent  bool
}

// withModule runs `use` as the capability on the long-lived instance. Only
// the logs' heads outlive the call (wasmhost.Held.Sequence).
func (ls *Logs) withModule(ctx context.Context, capability string, use func(*wasmhost.Host) error) error {
	return ls.long().Sequence(ctx, func(inst *wasmhost.Instance) error { return use(inst.As(capability)) })
}

func (ls *Logs) all() map[string]*Log {
	ls.once.Do(func() {
		ls.kept = map[string]*Log{}
		t, err := ls.table()
		if err != nil || ls.store == nil {
			return
		}
		for _, e := range t.Kinds {
			if e.Log == "" || e.Receive == "" {
				continue
			}
			l := ls.kept[e.Log]
			if l == nil {
				l = &Log{name: e.Log, receive: e.Receive, docs: map[string]logDoc{}}
				l.intent = e.Intent == nil || *e.Intent
				l.fold = &fold{name: e.Log, assemble: func(records map[string]json.RawMessage) error { return ls.assembleKept(l, records) }, documents: func() map[string]string {
					hashes := map[string]string{}
					for _, k := range l.kinds {
						hashes[k.StoreKind+"_hash"] = l.docs[k.StoreKind].hash
					}
					if len(l.kinds) == 1 {
						hashes["document"] = l.docs[l.kinds[0].StoreKind].hash
					}
					return hashes
				}}
				ls.kept[e.Log] = l
			}
			l.kinds = append(l.kinds, e)
			// A revert of an LWW kind writes its records back, stamped to win the
			// merge (D-193).
			if e.Merge == "lww" && e.Module != "" {
				module := e.Module
				l.fold.revise = func(states json.RawMessage, nowMs int64) (json.RawMessage, error) {
					var out struct {
						Written json.RawMessage `json:"written"`
					}
					ctx := context.Background()
					err := ls.withModule(ctx, module, func(host *wasmhost.Host) error {
						return host.Invoke(ctx, map[string]any{"op": "log_revert", "states": states, "now_ms": nowMs}, &out)
					})
					return out.Written, err
				}
			}
		}
	})
	return ls.kept
}

func ErrNoLog(name string) error {
	return fmt.Errorf("the kind table names no %s log (run `just caps`)", name)
}

func (ls *Logs) Get(name string) *Log {
	return ls.all()[name]
}

func (ls *Logs) Names() []string {
	var names []string
	for name := range ls.all() {
		names = append(names, name)
	}
	sort.Strings(names)
	return names
}
