package main

import (
	"context"
	"testing"
)

const testWasm = "../web/pkg/server.wasm"

// testView speaks as one capability to an instance of the server's
// module of its own, closed when the test ends.
func testView(t *testing.T, capability string) *WasmHost {
	t.Helper()
	inst, err := NewWasmHost(context.Background(), testWasm)
	if err != nil {
		t.Fatalf("the server's module (run `just caps`): %v", err)
	}
	t.Cleanup(func() { inst.Close(context.Background()) })
	return inst.As(capability)
}
