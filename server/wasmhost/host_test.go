package wasmhost

import (
	"context"
	"errors"
	"os"
	"sync"
	"testing"

	"github.com/tetratelabs/wazero"
)

// Callers that ask for the same bytes at the same time share one compile. A
// failed compile is not kept.
func TestCompiledModuleIsSharedAndFailuresAreNotKept(t *testing.T) {
	ctx := context.Background()
	body, err := os.ReadFile("../../web/pkg/server.wasm")
	if err != nil {
		t.Fatal(err)
	}
	const hash = "test:same-bytes"
	t.Cleanup(func() {
		compileMu.Lock()
		delete(compiled, hash)
		compileMu.Unlock()
	})
	const callers = 8
	modules := make([]wazero.CompiledModule, callers)
	var wg sync.WaitGroup
	for i := range modules {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			_, module, err := compiledModule(ctx, hash, body)
			if err != nil {
				t.Error(err)
				return
			}
			modules[i] = module
		}(i)
	}
	wg.Wait()
	for i, module := range modules {
		if module == nil || module != modules[0] {
			t.Fatalf("caller %d got another compiled module", i)
		}
	}

	const bad = "test:not-wasm"
	if _, _, err := compiledModule(ctx, bad, []byte("not wasm")); err == nil {
		t.Fatal("compiled bytes that are not wasm")
	}
	compileMu.Lock()
	_, kept := compiled[bad]
	compileMu.Unlock()
	if kept {
		t.Fatal("a failed compile was kept")
	}

	const waiting = "test:waiting"
	compileMu.Lock()
	compiled[waiting] = &compileEntry{done: make(chan struct{})}
	compileMu.Unlock()
	t.Cleanup(func() {
		compileMu.Lock()
		delete(compiled, waiting)
		compileMu.Unlock()
	})
	cancelled, cancel := context.WithCancel(ctx)
	cancel()
	if _, _, err := compiledModule(cancelled, waiting, body); !errors.Is(err, context.Canceled) {
		t.Fatalf("waiting with a cancelled context: %v", err)
	}
}
