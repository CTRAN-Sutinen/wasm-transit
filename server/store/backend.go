package store

import (
	"context"
	"errors"
	"sort"
	"strings"
	"sync"
	"time"
)

// Backend is everything the engine may assume of an object store
// (D-113; docs/units/store-durability.md, "The portable contract"): a
// whole-object create that lands entirely or not at all and succeeds
// only when the key is absent, a strongly consistent read and list
// after write, object metadata, delete, and an object's modified time.
// No append, no rename, no fsync, no lock. S3, Azure Blob and GCS all
// guarantee it; an endpoint that breaks it is unfit for the store, not
// a case to work around. Keys are '/'-separated; a backend scoped to a
// prefix strips it from every key it answers.
type Backend interface {
	// Get reads an object and its metadata; ErrNotFound when absent.
	Get(ctx context.Context, key string) ([]byte, map[string]string, error)
	// Stat reads an object's key and modified time; ErrNotFound when
	// absent.
	Stat(ctx context.Context, key string) (ObjectInfo, error)
	// Create writes an object only if its key is absent; ErrExists when
	// it is present (the object is then untouched). Metadata keys are
	// lowercase ASCII letters — the subset every provider keeps as given.
	Create(ctx context.Context, key string, data []byte, meta map[string]string) error
	// List answers every key under prefix that sorts after startAfter
	// ("" = from the start), ascending.
	List(ctx context.Context, prefix, startAfter string) ([]ObjectInfo, error)
	// Delete removes an object; an absent key is not an error.
	Delete(ctx context.Context, key string) error
}

// ObjectInfo is one listed object.
type ObjectInfo struct {
	Key      string
	Modified time.Time
}

// ErrExists is a conditional create over a present key.
var ErrExists = errors.New("store: object exists")

// Memory is the in-memory backend: `mem://`, what `just test` runs
// every store test over. It honors the contract exactly, and nothing
// outlives the process.
type Memory struct {
	mu   sync.Mutex
	objs map[string]memObject
	now  func() time.Time
}

type memObject struct {
	data     []byte
	meta     map[string]string
	modified time.Time
}

// NewMemory answers an empty in-memory backend.
func NewMemory() *Memory {
	return &Memory{objs: map[string]memObject{}, now: time.Now}
}

func (m *Memory) Get(_ context.Context, key string) ([]byte, map[string]string, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	o, ok := m.objs[key]
	if !ok {
		return nil, nil, ErrNotFound
	}
	meta := make(map[string]string, len(o.meta))
	for k, v := range o.meta {
		meta[k] = v
	}
	return append([]byte(nil), o.data...), meta, nil
}

func (m *Memory) Stat(_ context.Context, key string) (ObjectInfo, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	o, ok := m.objs[key]
	if !ok {
		return ObjectInfo{}, ErrNotFound
	}
	return ObjectInfo{Key: key, Modified: o.modified}, nil
}

func (m *Memory) Create(_ context.Context, key string, data []byte, meta map[string]string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	if _, ok := m.objs[key]; ok {
		return ErrExists
	}
	cp := make(map[string]string, len(meta))
	for k, v := range meta {
		cp[k] = v
	}
	m.objs[key] = memObject{data: append([]byte(nil), data...), meta: cp, modified: m.now()}
	return nil
}

func (m *Memory) List(_ context.Context, prefix, startAfter string) ([]ObjectInfo, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	var out []ObjectInfo
	for k, o := range m.objs {
		if strings.HasPrefix(k, prefix) && k > startAfter {
			out = append(out, ObjectInfo{Key: k, Modified: o.modified})
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Key < out[j].Key })
	return out, nil
}

func (m *Memory) Delete(_ context.Context, key string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	delete(m.objs, key)
	return nil
}

// age backdates an object's modified time (tests: an orphan past the
// grace period).
func (m *Memory) age(key string, by time.Duration) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if o, ok := m.objs[key]; ok {
		o.modified = o.modified.Add(-by)
		m.objs[key] = o
	}
}
