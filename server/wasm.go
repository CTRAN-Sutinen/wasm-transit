package main

// Only package wasmhost touches the module ABI. Everything above it
// speaks typed Go values. The server runs one module (`-wasm`, D-227) as
// the instances docs/architecture/server.md names: the long-lived one,
// authz's own, and a fresh one for each import or publish.

import (
	"context"
	_ "embed"
	"encoding/json"
	"log"
	"sync"
	"sync/atomic"

	"rustwasm/server/wasmhost"
)

type WasmHost = wasmhost.Host
type Instance = wasmhost.Instance

const defaultWasm = "../web/pkg/server.wasm"

// NewWasmHost compiles the module once for each process, and makes a new
// instance of it each call.
func NewWasmHost(ctx context.Context, path string) (*Instance, error) {
	return wasmhost.New(ctx, path)
}

const (
	opLoadNetwork = 256
	opLoadStreets = 263
	opLoadFeed    = 266
)

// The long-lived instance passes this, and a refresh replaces it.
const memoryCeiling = 256 << 20

// A held instance lives as long as the server. A trap, or memory past
// the ceiling, starts a refresh: a fresh instance of the same compiled
// module gets the heads' documents beside the held one, which answers
// until the swap.
type held struct {
	name    string
	cur     atomic.Pointer[Instance]
	ceiling uint64

	// seq holds a sequence of calls (load, op, evict) to one instance,
	// and the swap.
	seq sync.Mutex
	// heads are the documents a sequence registered as a log's head, by
	// log and kind. Every other document is evicted after its sequence.
	heads map[string]heldDoc
	// loads counts the heads a sequence registered, for tests.
	loads int

	refreshing atomic.Bool
	refreshes  sync.WaitGroup
	// hook runs at each stage of a refresh, for tests.
	hook func(stage string)
}

type heldDoc struct {
	store  string
	loaded string
	code   uint32
	doc    []byte
}

func newHeld(name string, inst *Instance) *held {
	m := &held{name: name, ceiling: memoryCeiling, heads: map[string]heldDoc{}}
	m.cur.Store(inst)
	return m
}

func (m *held) Current() *Instance { return m.cur.Load() }

func (m *held) as(capability string) *WasmHost { return wasmhost.Speak(m, capability) }

func (m *held) Trapped(*Instance) { m.refresh("a trap") }

// sequence runs `use` on the instance with no other sequence between its
// calls, then evicts every document that is not a held head.
func (m *held) sequence(ctx context.Context, use func(*Instance) error) error {
	m.seq.Lock()
	defer m.seq.Unlock()
	inst := m.Current()
	err := use(inst)
	keep := []string{}
	for _, h := range m.heads {
		keep = append(keep, h.loaded)
	}
	body, _ := json.Marshal(map[string]any{"keep": keep})
	var evicted json.RawMessage
	if evictErr := inst.InvokeBytes(ctx, opEvict, body, &evicted); evictErr != nil && err == nil {
		err = evictErr
	}
	switch {
	case inst.Trapped():
		m.refresh("a trap")
	case inst.MemoryBytes() > m.ceiling:
		m.refresh("memory past the ceiling")
	}
	return err
}

// head answers the loaded hash of a log's head, registering it only when
// the instance does not hold it. The caller holds seq.
func (m *held) head(ctx context.Context, inst *Instance, key, storeHash string, code uint32, doc []byte) (string, error) {
	if h, ok := m.heads[key]; ok && h.store == storeHash {
		return h.loaded, nil
	}
	var loaded map[string]any
	if err := inst.InvokeBytes(ctx, code, doc, &loaded); err != nil {
		return "", err
	}
	hash := loadedHash(loaded)
	m.loads++
	m.heads[key] = heldDoc{store: storeHash, loaded: hash, code: code, doc: doc}
	return hash, nil
}

// refresh never runs in a request's path. The held instance answers until
// the swap, then finishes the call it holds and closes.
func (m *held) refresh(why string) {
	if !m.refreshing.CompareAndSwap(false, true) {
		return
	}
	m.refreshes.Add(1)
	go func() {
		defer m.refreshes.Done()
		defer m.refreshing.Store(false)
		ctx := context.Background()
		old := m.Current()
		next, err := old.Fresh(ctx)
		if err != nil {
			log.Printf("the %s instance: a refresh after %s failed: %v", m.name, why, err)
			return
		}
		m.seq.Lock()
		heads := make(map[string]heldDoc, len(m.heads))
		for k, h := range m.heads {
			heads[k] = h
		}
		m.seq.Unlock()
		registered := m.register(ctx, next, heads, nil)
		m.stage("registered")
		m.seq.Lock()
		registered = m.register(ctx, next, m.heads, registered)
		for k, h := range m.heads {
			if registered[k] != h.store {
				delete(m.heads, k)
			}
		}
		m.cur.Store(next)
		m.seq.Unlock()
		m.stage("swapped")
		old.Close(ctx)
		log.Printf("the %s instance: replaced after %s (%d MB held)", m.name, why, old.MemoryBytes()>>20)
	}()
}

// register loads each head the next instance does not hold yet, and
// answers what it holds. A head it cannot load is left out, and the next
// sequence that needs it loads it.
func (m *held) register(ctx context.Context, next *Instance, heads map[string]heldDoc, already map[string]string) map[string]string {
	out := map[string]string{}
	for k, v := range already {
		out[k] = v
	}
	for k, h := range heads {
		if out[k] == h.store {
			continue
		}
		var loaded map[string]any
		if err := next.InvokeBytes(ctx, h.code, h.doc, &loaded); err != nil {
			delete(out, k)
			continue
		}
		out[k] = h.store
	}
	return out
}

func (m *held) stage(name string) {
	if m.hook != nil {
		m.hook(name)
	}
}

// The server's one module. Tests make a server around one instance
// (`wasm`), and every other instance comes from it.
func (s *server) longLived() *held {
	s.longOnce.Do(func() {
		if s.long == nil {
			s.long = newHeld("long-lived", s.wasm.Instance())
		}
	})
	return s.long
}

func (s *server) as(capability string) *WasmHost { return s.longLived().as(capability) }

// fresh is an instance of its own, for an import or a publish, so the
// memory it used goes with it. The caller closes it.
func (s *server) fresh(ctx context.Context) (*Instance, error) {
	return s.longLived().Current().Fresh(ctx)
}

// The capabilities the server calls by name. gen-assemblies links them
// into the server's module, with each one the kind list names.
//
//go:embed capabilities.json
var ownCapabilities []byte

func unlinked(inst *Instance) []string {
	var own []string
	json.Unmarshal(ownCapabilities, &own)
	var missing []string
	for _, id := range own {
		if !inst.Links(id) {
			missing = append(missing, id)
		}
	}
	return missing
}

// unhosted are the capabilities the kind table names that the module
// does not link.
func (s *server) unhosted() []string {
	t, err := s.kindTable()
	if err != nil {
		return nil
	}
	inst := s.longLived().Current()
	seen := map[string]bool{}
	var missing []string
	for _, e := range t.Kinds {
		for _, id := range []string{e.Module, e.Receive} {
			if id != "" && !seen[id] && !inst.Links(id) {
				missing = append(missing, id)
			}
			seen[id] = true
		}
	}
	return missing
}
