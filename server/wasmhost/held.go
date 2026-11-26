package wasmhost

import (
	"context"
	"encoding/json"
	"log"
	"strings"
	"sync"
	"sync/atomic"
)

// The kernel's ops that every module answers (registry kernel_ops).
const (
	OpLoadNetwork = 256
	OpEvict       = 257
	OpLoadStreets = 263
	OpLoadFeed    = 266
)

// MemoryCeiling is the memory a held instance passes before a refresh
// replaces it.
const MemoryCeiling = 256 << 20

// A Held instance lives as long as the server. A trap, or memory past
// the ceiling, starts a refresh: a fresh instance of the same compiled
// module gets the heads' documents beside the held one, which answers
// until the swap.
type Held struct {
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
	// Hook runs at each stage of a refresh, for tests.
	Hook func(stage string)
}

type heldDoc struct {
	store  string
	loaded string
	code   uint32
	doc    []byte
}

func NewHeld(name string, inst *Instance) *Held {
	m := &Held{name: name, ceiling: MemoryCeiling, heads: map[string]heldDoc{}}
	m.cur.Store(inst)
	return m
}

func (m *Held) Current() *Instance { return m.cur.Load() }

func (m *Held) As(capability string) *Host { return Speak(m, capability) }

func (m *Held) Trapped(*Instance) { m.refresh("a trap") }

// SetCeiling is for tests.
func (m *Held) SetCeiling(bytes uint64) {
	m.seq.Lock()
	m.ceiling = bytes
	m.seq.Unlock()
}

// Loads answers how many heads sequences registered, and how many the
// instance holds, for tests.
func (m *Held) Loads() (loads, heads int) {
	m.seq.Lock()
	defer m.seq.Unlock()
	return m.loads, len(m.heads)
}

// Locked runs use between sequences, with no call or eviction of its
// own, for tests.
func (m *Held) Locked(use func()) {
	m.seq.Lock()
	defer m.seq.Unlock()
	use()
}

// Wait returns when no refresh runs.
func (m *Held) Wait() { m.refreshes.Wait() }

// Sequence runs `use` on the instance with no other sequence between its
// calls, then evicts every document that is not a held head.
func (m *Held) Sequence(ctx context.Context, use func(*Instance) error) error {
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
	if evictErr := inst.InvokeBytes(ctx, OpEvict, body, &evicted); evictErr != nil && err == nil {
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

// Head answers the loaded hash of a log's head, registering it only when
// the instance does not hold it. The caller is inside Sequence.
func (m *Held) Head(ctx context.Context, inst *Instance, key, storeHash string, code uint32, doc []byte) (string, error) {
	if h, ok := m.heads[key]; ok && h.store == storeHash {
		return h.loaded, nil
	}
	var loaded map[string]any
	if err := inst.InvokeBytes(ctx, code, doc, &loaded); err != nil {
		return "", err
	}
	hash := LoadedHash(loaded)
	m.loads++
	m.heads[key] = heldDoc{store: storeHash, loaded: hash, code: code, doc: doc}
	return hash, nil
}

// LoadedHash is the document hash that a kernel load answers.
func LoadedHash(answer map[string]any) string {
	for k, v := range answer {
		if h, ok := v.(string); ok && strings.HasSuffix(k, "_hash") {
			return h
		}
	}
	return ""
}

// refresh never runs in a request's path. The held instance answers until
// the swap, then finishes the call it holds and closes.
func (m *Held) refresh(why string) {
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
func (m *Held) register(ctx context.Context, next *Instance, heads map[string]heldDoc, already map[string]string) map[string]string {
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

func (m *Held) stage(name string) {
	if m.Hook != nil {
		m.Hook(name)
	}
}
