package main

// The wazero host for a capability module. The module has zero
// imports and four exports (see crates/cap-abi/src/lib.rs for the ABI
// contract): staging buffers move through transit_alloc/transit_free,
// JSON requests through transit_invoke, bulk payloads through
// transit_invoke_bytes. Results are [u32 LE length][payload] buffers
// holding {"ok": ...} or {"error": ...}.
//
// This server hosts these modules: the SYNC CAPABILITY (cap-sync),
// the op behind POST /edits, held for the server's lifetime; the
// AUTHORIZATION CAPABILITY (cap-authz) when auth is on (authz.go); the
// PLANS CAPABILITY (cap-plans) in store mode, the op behind POST /plans
// (plans.go); the STOPS CAPABILITY (cap-stops) in store mode, one
// instance emptied after each use (stops.go); the NETWORK CAPABILITY
// (cap-network), instantiated once
// per published feed to import it — and once per published pick to
// validate and canonicalize it — and then closed (network.go,
// plans.go); and the STREETS
// CAPABILITY (cap-streets), instantiated once per published street
// layer (and network) to import it and derive the walk table, then
// closed (streets.go). Each module's bytes are compiled once per process
// ([compiledModule]); an instance is cheap (well under a millisecond),
// so a fresh one per import or publish costs only its memory. This file is the
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

// WasmHost wraps one instance of the artifact: its own linear memory,
// instantiated from the module compiled once for the process.
//
// The artifact is NOT reentrant: its heap and linear memory are
// single-instance state, and its internal Mutex only guards the feed
// registry map. Every invoke is therefore serialized here — one
// module instance, one call at a time (net/http serves each request
// on its own goroutine; without this lock concurrent /edits and
// /manifest handling would corrupt wasm memory).
type WasmHost struct {
	module      api.Module
	memory      api.Memory
	alloc       api.Function
	free        api.Function
	invoke      api.Function
	invokeBytes api.Function
	hash        string // sha256 of the loaded bytes — the manifest pins this

	mu sync.Mutex
}

// The process's one runtime and each artifact compiled in it, by the
// sha256 of its bytes: compiling (decoding, validating, generating
// machine code) is the whole cost of a module — 1–3 s for cap-network,
// many times that under the race detector — and an instance made from
// the compiled module costs well under a millisecond. Compiled modules
// live as long as the process; new bytes (a new build's modules) compile
// once more beside the old.
var (
	compileMu sync.Mutex
	shared    wazero.Runtime
	compiled  = map[string]wazero.CompiledModule{}
)

// compiledModule is the artifact's compiled module, compiling it the
// first time its bytes are seen.
func compiledModule(ctx context.Context, hash string, body []byte) (wazero.Runtime, wazero.CompiledModule, error) {
	compileMu.Lock()
	defer compileMu.Unlock()
	if shared == nil {
		shared = wazero.NewRuntime(context.Background())
	}
	if module, ok := compiled[hash]; ok {
		return shared, module, nil
	}
	module, err := shared.CompileModule(ctx, body)
	if err != nil {
		return nil, nil, err
	}
	compiled[hash] = module
	return shared, module, nil
}

// NewWasmHost loads the artifact and instantiates it. The bytes are
// hashed once: the manifest's wasm_hash claim is about exactly what
// runs here.
func NewWasmHost(ctx context.Context, artifactPath string) (*WasmHost, error) {
	body, err := os.ReadFile(artifactPath)
	if err != nil {
		return nil, err
	}
	sum := sha256.Sum256(body)
	hash := "sha256:" + hex.EncodeToString(sum[:])
	runtime, code, err := compiledModule(ctx, hash, body)
	if err != nil {
		return nil, fmt.Errorf("compile %s: %w", artifactPath, err)
	}
	// Zero imports: instantiation is the whole integration story. An
	// anonymous instance: any number share the runtime.
	module, err := runtime.InstantiateModule(ctx, code, wazero.NewModuleConfig().WithName(""))
	if err != nil {
		return nil, fmt.Errorf("instantiate %s: %w", artifactPath, err)
	}
	return &WasmHost{
		module:      module,
		memory:      module.Memory(),
		alloc:       module.ExportedFunction("transit_alloc"),
		free:        module.ExportedFunction("transit_free"),
		invoke:      module.ExportedFunction("transit_invoke"),
		invokeBytes: module.ExportedFunction("transit_invoke_bytes"),
		hash:        hash,
	}, nil
}

// Close releases the instance and its memory (the compiled module stays
// for the next instance).
func (h *WasmHost) Close(ctx context.Context) error {
	return h.module.Close(ctx)
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

// The kernel's loads this host calls (cap_abi::kernel — the same op in
// every module whose capabilities read the kind): raw bytes on
// transit_invoke_bytes.
const (
	opLoadNetwork = 256
	opLoadStreets = 263
	opLoadFeed    = 266
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
