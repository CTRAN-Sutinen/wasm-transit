package main

// The wazero host for a capability module. The module has zero
// imports and four exports (see crates/cap-abi/src/lib.rs for the ABI
// contract): staging buffers move through transit_alloc/transit_free,
// JSON requests through transit_invoke, bulk payloads through
// transit_invoke_bytes. Results are [u32 LE length][payload] buffers
// holding {"ok": ...} or {"error": ...}.
//
// This server hosts two modules: the SYNC CAPABILITY (cap-sync), the
// op behind POST /edits, held for the server's lifetime; and the
// NETWORK CAPABILITY (cap-network), instantiated once per published
// feed to import it and then closed (network.go). This file is the
// only place Go touches the ABI; everything above it speaks typed Go
// values.

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"sync"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"
)

// WasmHost wraps one instance of the artifact. The runtime is held for
// the host's lifetime — wazero ties module lifetime to it.
//
// The artifact is NOT reentrant: its heap and linear memory are
// single-instance state, and its internal Mutex only guards the feed
// registry map. Every invoke is therefore serialized here — one
// module instance, one call at a time (net/http serves each request
// on its own goroutine; without this lock concurrent /edits and
// /manifest handling would corrupt wasm memory).
type WasmHost struct {
	runtime     wazero.Runtime
	module      api.Module
	memory      api.Memory
	alloc       api.Function
	free        api.Function
	invoke      api.Function
	invokeBytes api.Function
	hash        string // sha256 of the loaded bytes — the manifest pins this

	mu sync.Mutex
}

// compiled caches machine code by module bytes for the process: a
// module instantiated again (cap-network, per published feed)
// compiles once.
var compiled = wazero.NewCompilationCache()

// NewWasmHost loads the artifact and instantiates it. The bytes are
// hashed once: the manifest's wasm_hash claim is about exactly what
// runs here.
func NewWasmHost(ctx context.Context, artifactPath string) (*WasmHost, error) {
	body, err := os.ReadFile(artifactPath)
	if err != nil {
		return nil, err
	}
	runtime := wazero.NewRuntimeWithConfig(ctx, wazero.NewRuntimeConfig().WithCompilationCache(compiled))

	// Zero imports: instantiation is the whole integration story.
	module, err := runtime.Instantiate(ctx, body)
	if err != nil {
		runtime.Close(ctx)
		return nil, fmt.Errorf("instantiate %s: %w", artifactPath, err)
	}
	sum := sha256.Sum256(body)
	return &WasmHost{
		runtime:     runtime,
		module:      module,
		memory:      module.Memory(),
		alloc:       module.ExportedFunction("transit_alloc"),
		free:        module.ExportedFunction("transit_free"),
		invoke:      module.ExportedFunction("transit_invoke"),
		invokeBytes: module.ExportedFunction("transit_invoke_bytes"),
		hash:        "sha256:" + hex.EncodeToString(sum[:]),
	}, nil
}

// Close releases the module and runtime.
func (h *WasmHost) Close(ctx context.Context) error {
	return h.runtime.Close(ctx)
}

// Hash returns the sha256 of the artifact this host runs.
func (h *WasmHost) Hash() string { return h.hash }

// Invoke runs a JSON op. `request` marshals to a serialized
// capability Request; `out` receives the unmarshalled ok-payload.
func (h *WasmHost) Invoke(ctx context.Context, request, out any) error {
	payload, err := json.Marshal(request)
	if err != nil {
		return err
	}
	raw, err := h.call(ctx, h.invoke, payload)
	if err != nil {
		return err
	}
	return h.decode(raw, out)
}

// The bulk ops this host calls (cap-network's OP_LOAD_FEED, the
// kernel's OP_LOAD_NETWORK): raw bytes on transit_invoke_bytes.
const (
	opLoadFeed    = 0
	opLoadNetwork = 256
)

// InvokeBytes runs a bulk op: `payload` crosses as raw bytes (a GTFS
// zip, a bare network document); `out` receives the ok-payload.
func (h *WasmHost) InvokeBytes(ctx context.Context, op uint32, payload []byte, out any) error {
	raw, err := h.call(ctx, h.invokeBytes, payload, uint64(op))
	if err != nil {
		return err
	}
	return h.decode(raw, out)
}

// InvokeRaw runs a JSON op and returns its ok-payload's bytes
// untouched — for answers the host serves without reading.
func (h *WasmHost) InvokeRaw(ctx context.Context, request any) (json.RawMessage, error) {
	payload, err := json.Marshal(request)
	if err != nil {
		return nil, err
	}
	raw, err := h.call(ctx, h.invoke, payload)
	if err != nil {
		return nil, err
	}
	var ok json.RawMessage
	return ok, h.decode(raw, &ok)
}

// call stages `payload` at an allocated pointer, calls `fn` (with an
// optional leading extra arg for the bytes-op code), reads the
// length-prefixed result, frees both buffers, and returns the payload
// of a successful {"ok": ...} outcome.
//
// The whole alloc→free sequence is atomic: the artifact is not
// reentrant, so interleaved calls would race on its heap.
func (h *WasmHost) call(ctx context.Context, fn api.Function, payload []byte, extra ...uint64) ([]byte, error) {
	h.mu.Lock()
	defer h.mu.Unlock()
	ptr, err := h.alloc.Call(ctx, uint64(len(payload)))
	if err != nil {
		return nil, fmt.Errorf("alloc: %w", err)
	}
	in := uint32(ptr[0])
	if !h.memory.Write(in, payload) {
		return nil, fmt.Errorf("memory write at %d failed", in)
	}
	args := append(extra, uint64(in), uint64(len(payload)))
	out, err := fn.Call(ctx, args...)
	if err != nil {
		return nil, err
	}
	result := uint32(out[0])
	length, ok := h.memory.ReadUint32Le(result)
	if !ok {
		return nil, fmt.Errorf("result length read at %d failed", result)
	}
	body, ok := h.memory.Read(result+4, length)
	if !ok {
		return nil, fmt.Errorf("result read at %d (%d bytes) failed", result+4, length)
	}
	// Read returns a view into linear memory; the frees below would
	// let the Rust allocator reuse (and overwrite) that region, so
	// the payload must leave the module's memory before freeing.
	resultCopy := make([]byte, len(body))
	copy(resultCopy, body)
	if _, err := h.free.Call(ctx, uint64(result), uint64(length+4)); err != nil {
		return nil, fmt.Errorf("free result: %w", err)
	}
	if _, err := h.free.Call(ctx, uint64(in), uint64(len(payload))); err != nil {
		return nil, fmt.Errorf("free input: %w", err)
	}
	return resultCopy, nil
}

// decode parses an outcome; {"error": ...} becomes a Go error.
func (h *WasmHost) decode(raw []byte, out any) error {
	var outcome struct {
		OK    json.RawMessage `json:"ok"`
		Error *string         `json:"error"`
	}
	if err := json.Unmarshal(raw, &outcome); err != nil {
		return fmt.Errorf("unparseable outcome: %w", err)
	}
	if outcome.Error != nil {
		return fmt.Errorf("op failed: %s", *outcome.Error)
	}
	if out == nil || len(outcome.OK) == 0 {
		return nil
	}
	return json.Unmarshal(outcome.OK, out)
}

func mustMarshal(v any) []byte {
	b, err := json.Marshal(v)
	if err != nil {
		panic(err) // map literals with string keys cannot fail
	}
	return b
}
