package main

// The settings file (-config): the server's flags written down, so a
// deployment or a developer keeps them in one file instead of a long
// command line. TOML, because people edit it and it takes comments.
//
// Each key is a flag's name; a [table] prefixes its keys with its name
// and a dash ([oidc] issuer is -oidc-issuer). A value is a string, true
// or false, or a whole number (a duration is a string: "5s"). A relative
// path is read from the file's directory, so a file means the same thing
// from wherever the server starts; a URL, a bucket:<key> source, or an
// absolute path passes as given. -config repeats: a later file is laid
// over an earlier, and a flag given on the command line beats every file.
// A key that names no flag, or a value its flag refuses, stops the boot
// with the file's name — a misspelt setting is never silently ignored.

import (
	"flag"
	"fmt"
	"path/filepath"
	"sort"
	"strings"

	"github.com/BurntSushi/toml"
)

// configList is -config: the settings files, in the order given.
type configList []string

func (c *configList) String() string     { return strings.Join(*c, ",") }
func (c *configList) Set(v string) error { *c = append(*c, v); return nil }

// pathSettings are the flags whose value may be a local path; a relative
// one is read from the settings file's directory.
var pathSettings = map[string]bool{
	"dist": true, "wasm": true, "feed": true, "network-wasm": true,
	"streets": true, "streets-wasm": true, "edits": true, "migrate-wasm": true,
	"plans-wasm": true, "requests-wasm": true, "stops-wasm": true,
	"authz-wasm": true, "oidc-key": true, "policy": true,
	"geocoder-policy": true, "export": true, "live-feeds": true,
}

// applyConfig sets each flag the command line left alone from the files,
// later files over earlier ones.
func applyConfig(fs *flag.FlagSet, files []string) error {
	given := map[string]bool{}
	fs.Visit(func(f *flag.Flag) { given[f.Name] = true })
	for _, file := range files {
		var raw map[string]any
		if _, err := toml.DecodeFile(file, &raw); err != nil {
			return fmt.Errorf("%s: %w", file, err)
		}
		flat := map[string]any{}
		if err := flatten("", raw, flat); err != nil {
			return fmt.Errorf("%s: %w", file, err)
		}
		keys := make([]string, 0, len(flat))
		for k := range flat {
			keys = append(keys, k)
		}
		sort.Strings(keys)
		for _, key := range keys {
			if key == "config" {
				return fmt.Errorf("%s: a settings file cannot name another (config); give each with -config", file)
			}
			if fs.Lookup(key) == nil {
				return fmt.Errorf("%s: %q is not a setting (each key is a flag's name; -help lists them)", file, key)
			}
			if given[key] {
				continue
			}
			value, err := settingText(flat[key])
			if err != nil {
				return fmt.Errorf("%s: %s: %w", file, key, err)
			}
			if pathSettings[key] && isLocalPath(value) {
				value = filepath.Join(filepath.Dir(file), value)
			}
			if err := fs.Set(key, value); err != nil {
				return fmt.Errorf("%s: %s: %w", file, key, err)
			}
		}
	}
	return nil
}

func flatten(prefix string, in map[string]any, out map[string]any) error {
	for k, v := range in {
		name := k
		if prefix != "" {
			name = prefix + "-" + k
		}
		if table, ok := v.(map[string]any); ok {
			if err := flatten(name, table, out); err != nil {
				return err
			}
			continue
		}
		out[name] = v
	}
	return nil
}

func settingText(v any) (string, error) {
	switch x := v.(type) {
	case string:
		return x, nil
	case bool:
		return fmt.Sprint(x), nil
	case int64:
		return fmt.Sprint(x), nil
	}
	return "", fmt.Errorf("a value is a string, true or false, or a whole number (quote a date or a duration), not %T", v)
}

func isLocalPath(v string) bool {
	return v != "" && !filepath.IsAbs(v) && !strings.Contains(v, "://") && !strings.HasPrefix(v, "bucket:")
}
