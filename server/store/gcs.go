package store

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"

	"cloud.google.com/go/storage"
	"google.golang.org/api/googleapi"
	"google.golang.org/api/iterator"
)

// GCS is the backend over Google Cloud Storage. The conditional create
// is an upload with ifGenerationMatch=0.
type GCS struct {
	b      *storage.BucketHandle
	prefix string // "" or "<prefix>/"
}

// NewGCS answers the backend over bucket, every key under prefix.
// Credentials are Application Default Credentials;
// STORAGE_EMULATOR_HOST names a local emulator.
func NewGCS(ctx context.Context, bucket, prefix string) (*GCS, error) {
	c, err := storage.NewClient(ctx)
	if err != nil {
		return nil, fmt.Errorf("gcs: %w", err)
	}
	if prefix != "" {
		prefix += "/"
	}
	return &GCS{b: c.Bucket(bucket), prefix: prefix}, nil
}

func gcsCode(err error) int {
	var ge *googleapi.Error
	if errors.As(err, &ge) {
		return ge.Code
	}
	return 0
}

func (g *GCS) Get(ctx context.Context, key string) ([]byte, map[string]string, error) {
	obj := g.b.Object(g.prefix + key)
	r, err := obj.NewReader(ctx)
	if err != nil {
		if errors.Is(err, storage.ErrObjectNotExist) {
			return nil, nil, ErrNotFound
		}
		return nil, nil, err
	}
	defer r.Close()
	data, err := io.ReadAll(r)
	if err != nil {
		return nil, nil, err
	}
	// The reader carries no metadata; read it at the generation just read,
	// so the two halves are one object.
	attrs, err := obj.Generation(r.Attrs.Generation).Attrs(ctx)
	if err != nil {
		return nil, nil, err
	}
	meta := map[string]string{}
	for k, v := range attrs.Metadata {
		meta[strings.ToLower(k)] = v
	}
	return data, meta, nil
}

func (g *GCS) Stat(ctx context.Context, key string) (ObjectInfo, error) {
	attrs, err := g.b.Object(g.prefix + key).Attrs(ctx)
	if err != nil {
		if errors.Is(err, storage.ErrObjectNotExist) {
			return ObjectInfo{}, ErrNotFound
		}
		return ObjectInfo{}, err
	}
	return ObjectInfo{Key: key, Modified: attrs.Updated}, nil
}

func (g *GCS) Create(ctx context.Context, key string, data []byte, meta map[string]string) error {
	w := g.b.Object(g.prefix + key).If(storage.Conditions{DoesNotExist: true}).NewWriter(ctx)
	w.Metadata = meta
	w.ChunkSize = 0 // one request: the object lands whole or not at all
	if _, err := w.Write(data); err != nil {
		w.Close()
		return err
	}
	err := w.Close()
	if gcsCode(err) == http.StatusPreconditionFailed {
		return ErrExists
	}
	return err
}

func (g *GCS) List(ctx context.Context, prefix, startAfter string) ([]ObjectInfo, error) {
	q := &storage.Query{Prefix: g.prefix + prefix}
	if startAfter != "" {
		q.StartOffset = g.prefix + startAfter // inclusive: filtered below
	}
	q.SetAttrSelection([]string{"Name", "Updated"})
	var out []ObjectInfo
	it := g.b.Objects(ctx, q)
	for {
		attrs, err := it.Next()
		if errors.Is(err, iterator.Done) {
			break
		}
		if err != nil {
			return nil, err
		}
		key := strings.TrimPrefix(attrs.Name, g.prefix)
		if key <= startAfter {
			continue
		}
		out = append(out, ObjectInfo{Key: key, Modified: attrs.Updated})
	}
	return out, nil
}

func (g *GCS) Delete(ctx context.Context, key string) error {
	err := g.b.Object(g.prefix + key).Delete(ctx)
	if errors.Is(err, storage.ErrObjectNotExist) {
		return nil
	}
	return err
}
