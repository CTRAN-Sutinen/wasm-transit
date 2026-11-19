package store

// Dump writes a whole store to a folder, to read it with no object
// storage tools (`store dump`): every object at <dir>/<key>, as its
// exact bytes, and its metadata, when it has any (a blob's envelope),
// beside it at <dir>/<key>.meta.json. It reads the store and writes
// nothing to it. A folder is never a store: nothing reads a dump back.

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
)

// DumpReport is what a dump wrote.
type DumpReport struct {
	Dir     string `json:"dir"`
	Objects int    `json:"objects"`
	Bytes   int64  `json:"bytes"`
}

// Dump writes every object of the store to dir, which must be absent or
// empty, so a dump never mixes with other files.
func Dump(ctx context.Context, b Backend, dir string) (DumpReport, error) {
	r := DumpReport{Dir: dir}
	if entries, err := os.ReadDir(dir); err == nil && len(entries) > 0 {
		return r, fmt.Errorf("store dump: %s is not empty", dir)
	} else if err != nil && !errors.Is(err, fs.ErrNotExist) {
		return r, err
	}
	objs, err := b.List(ctx, "", "")
	if err != nil {
		return r, err
	}
	for _, o := range objs {
		if o.Key == "" || strings.HasPrefix(o.Key, "/") || strings.Contains(o.Key, "..") {
			return r, fmt.Errorf("store dump: %q is not a key a folder can hold", o.Key)
		}
		data, meta, err := b.Get(ctx, o.Key)
		if err != nil {
			return r, fmt.Errorf("store dump: %s: %w", o.Key, err)
		}
		path := filepath.Join(dir, filepath.FromSlash(o.Key))
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			return r, err
		}
		if err := os.WriteFile(path, data, 0o644); err != nil {
			return r, err
		}
		if len(meta) > 0 {
			readable := map[string]any{}
			for k, v := range meta {
				var parsed any
				if json.Unmarshal([]byte(v), &parsed) == nil {
					readable[k] = parsed
				} else {
					readable[k] = v
				}
			}
			out, _ := json.MarshalIndent(readable, "", "  ")
			if err := os.WriteFile(path+".meta.json", append(out, '\n'), 0o644); err != nil {
				return r, err
			}
		}
		r.Objects++
		r.Bytes += int64(len(data))
	}
	return r, nil
}
