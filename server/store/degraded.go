package store

import (
	"context"
	"errors"
	"fmt"
	"sync"
	"time"
)

// ErrUnavailable is a bucket that did not answer (D-119): the network,
// a timeout, the provider's 5xx, credentials it refused. A read the
// store holds a cached answer for serves it, labelled stale; a write is
// refused — never acknowledged — and the caller says so (503).
var ErrUnavailable = errors.New("store: the bucket is not answering")

// callTimeout bounds one backend call, so a bucket that hangs is a
// bucket that does not answer; a create carries a whole blob, so it gets
// longer.
const (
	callTimeout   = 20 * time.Second
	createTimeout = 2 * time.Minute
)

// guarded classifies a backend's failures: ErrNotFound, ErrExists and a
// serving store's refusal pass through; anything else is unavailable.
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

// BlobCacheBytes bounds the blobs a store keeps in memory: the ones a
// degraded server can still serve. A C-TRAN pick (network 9.6 MB,
// streets 22 MB) and a long edit history fit many times over.
const BlobCacheBytes = 256 << 20

// blobCache holds blobs by hash, oldest out first past the bound.
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

// drop forgets a blob (GC deleted it).
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
