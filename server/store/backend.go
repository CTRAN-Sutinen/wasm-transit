package store

import (
	"context"
	"errors"
	"fmt"
	"net/url"
	"sort"
	"strings"
	"sync"
	"time"
)

// OpenBackend opens the backend a store URL names:
//
//	mem://                                       in memory (dev, tests)
//	s3://bucket[/prefix][?endpoint=URL&region=R]  S3 or any S3-compatible
//	                                             endpoint (path-style when
//	                                             endpoint is given)
//	gs://bucket[/prefix]                         Google Cloud Storage
//
// awscreds.go and gcpauth.go name the credential sources each reads.
// There is no directory backend: a store needs the object contract.
func OpenBackend(ctx context.Context, raw string) (Backend, error) {
	u, err := url.Parse(raw)
	if err != nil {
		return nil, fmt.Errorf("store URL %q: %w", raw, err)
	}
	switch u.Scheme {
	case "mem":
		return NewMemory(), nil
	case "s3":
		if u.Host == "" {
			return nil, fmt.Errorf("store URL %q names no bucket", raw)
		}
		q := u.Query()
		return NewS3(ctx, u.Host, strings.Trim(u.Path, "/"), q.Get("endpoint"), q.Get("region"))
	case "gs":
		if u.Host == "" {
			return nil, fmt.Errorf("store URL %q names no bucket", raw)
		}
		return NewGCS(ctx, u.Host, strings.Trim(u.Path, "/"))
	default:
		return nil, fmt.Errorf("store URL %q: want s3://, gs:// or mem:// (the store lives in object storage)", raw)
	}
}

// Backend is everything the engine may assume of an object store
// (docs/architecture/storage.md). S3 and GCS both guarantee it,
// and an endpoint that breaks it is unfit for the store. Keys are
// '/'-separated, and a backend scoped to a prefix strips it from every
// key it answers.
type Backend interface {
	Get(ctx context.Context, key string) ([]byte, map[string]string, error)
	Stat(ctx context.Context, key string) (ObjectInfo, error)
	// Create leaves a present object untouched and answers ErrExists.
	// Metadata keys are lowercase ASCII letters, the subset that every
	// provider keeps as given.
	Create(ctx context.Context, key string, data []byte, meta map[string]string) error
	// List answers keys in ascending order. "" for startAfter lists from
	// the start.
	List(ctx context.Context, prefix, startAfter string) ([]ObjectInfo, error)
	// Delete answers nil for an absent key.
	Delete(ctx context.Context, key string) error
}

type ObjectInfo struct {
	Key      string
	Modified time.Time
}

var ErrExists = errors.New("store: object exists")

// Memory is the `mem://` backend that `just test` runs every store test
// over.
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

// age backdates an object's modified time, so a test can age an orphan
// past the grace period.
func (m *Memory) age(key string, by time.Duration) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if o, ok := m.objs[key]; ok {
		o.modified = o.modified.Add(-by)
		m.objs[key] = o
	}
}
