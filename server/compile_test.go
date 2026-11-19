package main

// The test binary's module compiles. Compiling a module is most of a
// server test's cost — cap-network alone is ~16 s under the race
// detector, ~40 s for the server's modules one after another — so
// TestMain does two things before any test runs:
//
//   - It keeps compiled machine code on disk between runs (wazero's
//     compilation cache), keyed by the module's bytes, the wazero
//     version, and the CPU: a module whose bytes did not change since
//     the last run loads in well under a second. An entry is written
//     whole (temp file, fsync, rename) and checksummed on read, so a
//     damaged entry fails the compile loudly, never runs other code.
//     WASM_TEST_CACHE=off compiles every module afresh (the wazero
//     compiler then runs under -race too); WASM_TEST_CACHE=DIR moves
//     the cache. Entries untouched for two weeks are removed.
//   - It compiles every module in ../web/pkg at the same time, so the
//     first test to use a module finds it ready.
//
// The server itself keeps no disk cache: these settings live only in
// the test binary.
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

// testCompileCacheDir is where compiled modules are kept between runs,
// or "" for none.
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

// pruneCompileCache removes cache files not written for maxAge: a
// module rebuilt since leaves its old entry behind, and nothing else
// would remove it.
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

// prewarmModules compiles every server module at the same time. A
// module that fails to load is left for the test that uses it to
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
