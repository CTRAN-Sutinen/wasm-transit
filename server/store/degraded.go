package store

import (
	"context"
	"errors"
	"fmt"
	"sync"
	"time"
)

// ErrUnavailable covers the network, a timeout, the provider's 5xx, and
// refused credentials (D-119). A read with a cached answer serves it,
// labelled stale. A write is refused, never acknowledged, and the caller
// says so (503).
var ErrUnavailable = errors.New("store: the bucket is not answering")

// A create carries a whole blob, so it gets a longer timeout. They are
// variables so that a test can shorten them.
var (
	callTimeout   = 20 * time.Second
	createTimeout = 2 * time.Minute
)

// SetCallTimeouts is for the host contract suite
// (server/hostcontract_test.go). It answers the function that restores
// them.
func SetCallTimeouts(call, create time.Duration) (restore func()) {
	oldCall, oldCreate := callTimeout, createTimeout
	callTimeout, createTimeout = call, create
	return func() { callTimeout, createTimeout = oldCall, oldCreate }
}

// ErrNotFound, ErrExists, and a serving store's refusal pass through.
// Any other failure is unavailable.
type guarded struct{ b Backend }

func classify(err error) error {
	if err == nil || errors.Is(err, ErrNotFound) || errors.Is(err, ErrExists) || errors.Is(err, ErrServingNeverDeletes) {
		return err
	}
	return fmt.Errorf("%w: %v", ErrUnavailable, err)
}

func (g guarded) Get(ctx context.Context, k string) ([]byte, map[string]string, error) {
	ctx, cancel := context.WithTimeout(ctx, callTimeout)
	defer cancel()
	d, m, err := g.b.Get(ctx, k)
	return d, m, classify(err)
}

func (g guarded) Stat(ctx context.Context, k string) (ObjectInfo, error) {
	ctx, cancel := context.WithTimeout(ctx, callTimeout)
	defer cancel()
	o, err := g.b.Stat(ctx, k)
	return o, classify(err)
}

func (g guarded) Create(ctx context.Context, k string, d []byte, m map[string]string) error {
	ctx, cancel := context.WithTimeout(ctx, createTimeout)
	defer cancel()
	return classify(g.b.Create(ctx, k, d, m))
}

func (g guarded) List(ctx context.Context, p, a string) ([]ObjectInfo, error) {
	ctx, cancel := context.WithTimeout(ctx, callTimeout)
	defer cancel()
	o, err := g.b.List(ctx, p, a)
	return o, classify(err)
}

func (g guarded) Delete(ctx context.Context, k string) error {
	ctx, cancel := context.WithTimeout(ctx, callTimeout)
	defer cancel()
	return classify(g.b.Delete(ctx, k))
}

// A C-TRAN pick (network 9.6 MB, streets 22 MB) and a long edit history
// fit many times over.
const BlobCacheBytes = 256 << 20

// Blobs are immutable, so a cached one is always correct.
type blobCache struct {
	mu    sync.Mutex
	max   int
	total int
	order []string
	m     map[string]cachedBlob
}

type cachedBlob struct {
	data []byte
	env  Envelope
}

func newBlobCache(max int) *blobCache {
	return &blobCache{max: max, m: map[string]cachedBlob{}}
}

func (c *blobCache) get(hash string) (cachedBlob, bool) {
	c.mu.Lock()
	defer c.mu.Unlock()
	b, ok := c.m[hash]
	return b, ok
}

func (c *blobCache) put(hash string, data []byte, env Envelope) {
	if len(data) > c.max {
		return
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if _, ok := c.m[hash]; ok {
		return
	}
	for c.total+len(data) > c.max && len(c.order) > 0 {
		old := c.order[0]
		c.order = c.order[1:]
		c.total -= len(c.m[old].data)
		delete(c.m, old)
	}
	c.m[hash] = cachedBlob{data: data, env: env}
	c.order = append(c.order, hash)
	c.total += len(data)
}

func (c *blobCache) drop(hash string) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if b, ok := c.m[hash]; ok {
		c.total -= len(b.data)
		delete(c.m, hash)
		for i, h := range c.order {
			if h == hash {
				c.order = append(c.order[:i], c.order[i+1:]...)
				break
			}
		}
	}
}
