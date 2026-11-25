package main

// Compiling a module is most of a server test's cost. cap-network alone
// takes about 16 s under the race detector. So TestMain keeps wazero's
// compilation cache on disk between runs, keyed by the module's bytes,
// the wazero version, and the CPU, and compiles every module in
// ../web/pkg at the same time. An entry is written whole and
// checksummed on read, so a damaged entry fails the compile and never
// runs other code. WASM_TEST_CACHE=off compiles every module afresh.
// WASM_TEST_CACHE=DIR moves the cache. Entries untouched for two weeks
// are removed. The server itself keeps no disk cache.
//
// TestMain also puts the HTTP contract's check in front of every
// request a test sends (contract_test.go), and fails the run when the
// contract does not admit what a test saw.

import (
	"context"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sync"
	"testing"
	"time"

	"github.com/tetratelabs/wazero"

	"rustwasm/server/wasmhost"
)

func TestMain(m *testing.M) {
	ctx := context.Background()
	config := wazero.NewRuntimeConfig()
	if dir := testCompileCacheDir(); dir != "" {
		pruneCompileCache(dir, 14*24*time.Hour)
		cache, err := wazero.NewCompilationCacheWithDir(dir)
		if err != nil {
			fmt.Fprintf(os.Stderr, "compile cache %s: %v (compiling without it)\n", dir, err)
		} else {
			config = config.WithCompilationCache(cache)
		}
	}
	wasmhost.UseRuntime(wazero.NewRuntimeWithConfig(ctx, config))

	prewarmModules(ctx)
	if err := installContractCheck(); err != nil {
		fmt.Fprintf(os.Stderr, "the HTTP contract: %v\n", err)
		os.Exit(1)
	}
	code := m.Run()
	if !reportContract() && code == 0 {
		code = 1
	}
	os.Exit(code)
}

func testCompileCacheDir() string {
	switch v := os.Getenv("WASM_TEST_CACHE"); v {
	case "off":
		return ""
	case "":
		base, err := os.UserCacheDir()
		if err != nil {
			return ""
		}
		return filepath.Join(base, "rustwasm-server-test", "wazero")
	default:
		return v
	}
}

// pruneCompileCache removes the old entry that a rebuilt module leaves
// behind.
func pruneCompileCache(dir string, maxAge time.Duration) {
	cutoff := time.Now().Add(-maxAge)
	_ = filepath.WalkDir(dir, func(path string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return nil
		}
		if info, err := d.Info(); err == nil && info.ModTime().Before(cutoff) {
			_ = os.Remove(path)
		}
		return nil
	})
}

// A module that fails to load is left for the test that uses it to
// report.
func prewarmModules(ctx context.Context) {
	paths, _ := filepath.Glob("../web/pkg/*.wasm")
	var wg sync.WaitGroup
	for _, path := range paths {
		wg.Add(1)
		go func(path string) {
			defer wg.Done()
			if host, err := NewWasmHost(ctx, path); err == nil {
				host.Close(ctx)
			}
		}(path)
	}
	wg.Wait()
}
