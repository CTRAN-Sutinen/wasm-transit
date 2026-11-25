package wasmhost

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

// A module is not reentrant: its heap and linear memory are
// single-instance state. Every invoke is serialized here, one call at a
// time. net/http serves each request on its own goroutine, and concurrent
// calls would corrupt wasm memory.
type Host struct {
	module      api.Module
	memory      api.Memory
	alloc       api.Function
	free        api.Function
	invoke      api.Function
	invokeBytes api.Function
	hash        string

	mu sync.Mutex
}

// Compiled modules, by the sha256 of their bytes. Compiling costs 1 to 3
// s for cap-network, many times that under the race detector. An instance
// from the compiled module costs well under a millisecond.
//
// The lock guards only the map. Each module compiles under its own entry,
// so different modules compile at the same time and a caller never waits
// behind another module's compile. A failed compile is not kept.
var (
	compileMu sync.Mutex
	shared    wazero.Runtime
	compiled  = map[string]*compileEntry{}
)

type compileEntry struct {
	done   chan struct{}
	module wazero.CompiledModule
	err    error
}

func compiledModule(ctx context.Context, hash string, body []byte) (wazero.Runtime, wazero.CompiledModule, error) {
	compileMu.Lock()
	if shared == nil {
		shared = wazero.NewRuntime(context.Background())
	}
	runtime := shared
	entry, ok := compiled[hash]
	if !ok {
		entry = &compileEntry{done: make(chan struct{})}
		compiled[hash] = entry
	}
	compileMu.Unlock()

	if ok {
		select {
		case <-entry.done:
		case <-ctx.Done():
			return nil, nil, ctx.Err()
		}
	} else {
		entry.module, entry.err = runtime.CompileModule(ctx, body)
		if entry.err != nil {
			compileMu.Lock()
			delete(compiled, hash)
			compileMu.Unlock()
		}
		close(entry.done)
	}
	if entry.err != nil {
		return nil, nil, entry.err
	}
	return runtime, entry.module, nil
}

// New hashes the bytes once, so the manifest's wasm_hash names exactly
// what runs here.
func New(ctx context.Context, artifactPath string) (*Host, error) {
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
	// The name is empty so that any number of instances share the runtime.
	module, err := runtime.InstantiateModule(ctx, code, wazero.NewModuleConfig().WithName(""))
	if err != nil {
		return nil, fmt.Errorf("instantiate %s: %w", artifactPath, err)
	}
	return &Host{
		module:      module,
		memory:      module.Memory(),
		alloc:       module.ExportedFunction("transit_alloc"),
		free:        module.ExportedFunction("transit_free"),
		invoke:      module.ExportedFunction("transit_invoke"),
		invokeBytes: module.ExportedFunction("transit_invoke_bytes"),
		hash:        hash,
	}, nil
}

func (h *Host) Close(ctx context.Context) error {
	return h.module.Close(ctx)
}

func (h *Host) Hash() string { return h.hash }

func (h *Host) Invoke(ctx context.Context, request, out any) error {
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

const (
	OpLoadNetwork = 256
	OpLoadStreets = 263
	OpLoadFeed    = 266
)

func (h *Host) InvokeBytes(ctx context.Context, op uint32, payload []byte, out any) error {
	raw, err := h.call(ctx, h.invokeBytes, payload, uint64(op))
	if err != nil {
		return err
	}
	return h.decode(raw, out)
}

// InvokeRaw returns the ok-payload's bytes untouched, for answers the host
// serves without reading.
func (h *Host) InvokeRaw(ctx context.Context, request any) (json.RawMessage, error) {
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

// The whole alloc and free sequence holds the lock, because interleaved
// calls would race on the module's heap.
func (h *Host) call(ctx context.Context, fn api.Function, payload []byte, extra ...uint64) ([]byte, error) {
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
	// Read returns a view into linear memory. The frees below let the
	// Rust allocator overwrite that region, so copy the payload first.
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

func (h *Host) decode(raw []byte, out any) error {
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

// UseRuntime makes r the runtime for every later compile, such as a test
// binary's runtime with a compilation cache. Call it before the first New.
func UseRuntime(r wazero.Runtime) {
	compileMu.Lock()
	defer compileMu.Unlock()
	shared = r
}
