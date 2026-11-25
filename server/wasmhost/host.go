package wasmhost

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"sync"
	"sync/atomic"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"
)

// An Instance is one instance of a module. Its heap and linear memory are
// single-instance state, so every call is serialized here, one at a time.
// net/http serves each request on its own goroutine, and concurrent calls
// would corrupt wasm memory.
type Instance struct {
	module      api.Module
	memory      api.Memory
	alloc       api.Function
	free        api.Function
	invoke      api.Function
	invokeBytes api.Function
	code        *Module
	id          uint64
	trapped     atomic.Bool

	mu     sync.Mutex
	closed bool
}

// A Module is a compiled module and what its `about` op answers: the
// capabilities it links, in index order, and the kinds each one declares.
type Module struct {
	runtime   wazero.Runtime
	code      wazero.CompiledModule
	hash      string
	caps      map[string]uint32
	documents map[string]map[string]bool
}

// Compiled modules, by the sha256 of their bytes. Compiling costs 1 to 3
// s for the network capability, many times that under the race detector.
// An instance from the compiled module costs well under a millisecond.
//
// The lock guards only the map. Each module compiles under its own entry,
// so different modules compile at the same time and a caller never waits
// behind another module's compile. A failed compile is not kept.
var (
	compileMu sync.Mutex
	shared    wazero.Runtime
	compiled  = map[string]*compileEntry{}

	instances atomic.Int64
	ids       atomic.Uint64
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

// Live counts the instances open in this process.
func Live() int64 { return instances.Load() }

const OpAbout = 268

// New compiles the module at the path (once for each process) and makes
// its first instance. It hashes the bytes once, so the manifest's
// wasm_hash names exactly what runs here.
func New(ctx context.Context, artifactPath string) (*Instance, error) {
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
	m := &Module{runtime: runtime, code: code, hash: hash}
	inst, err := m.instantiate(ctx)
	if err != nil {
		return nil, fmt.Errorf("instantiate %s: %w", artifactPath, err)
	}
	var about struct {
		Capabilities []struct {
			ID        string   `json:"id"`
			Documents []string `json:"documents"`
		} `json:"capabilities"`
	}
	if err := inst.InvokeBytes(ctx, OpAbout, nil, &about); err != nil {
		inst.Close(ctx)
		return nil, fmt.Errorf("%s about: %w", artifactPath, err)
	}
	m.caps = map[string]uint32{}
	m.documents = map[string]map[string]bool{}
	for i, c := range about.Capabilities {
		m.caps[c.ID] = uint32(i)
		m.documents[c.ID] = map[string]bool{}
		for _, d := range c.Documents {
			m.documents[c.ID][d] = true
		}
	}
	return inst, nil
}

func (m *Module) instantiate(ctx context.Context) (*Instance, error) {
	// The name is empty so that any number of instances share the runtime.
	module, err := m.runtime.InstantiateModule(ctx, m.code, wazero.NewModuleConfig().WithName(""))
	if err != nil {
		return nil, err
	}
	instances.Add(1)
	return &Instance{
		module:      module,
		memory:      module.Memory(),
		alloc:       module.ExportedFunction("transit_alloc"),
		free:        module.ExportedFunction("transit_free"),
		invoke:      module.ExportedFunction("transit_invoke"),
		invokeBytes: module.ExportedFunction("transit_invoke_bytes"),
		code:        m,
		id:          ids.Add(1),
	}, nil
}

// Fresh makes another instance of the same compiled module, with its own
// memory.
func (i *Instance) Fresh(ctx context.Context) (*Instance, error) {
	return i.code.instantiate(ctx)
}

// Close waits for the call the instance holds.
func (i *Instance) Close(ctx context.Context) error {
	i.mu.Lock()
	defer i.mu.Unlock()
	if i.closed {
		return nil
	}
	i.closed = true
	instances.Add(-1)
	return i.module.Close(ctx)
}

func (i *Instance) Hash() string { return i.code.hash }

// ID tells one instance from another, for logs and tests.
func (i *Instance) ID() uint64 { return i.id }

// MemoryBytes is the linear memory's size, which never shrinks: the
// instance's high-water mark.
func (i *Instance) MemoryBytes() uint64 { return uint64(i.memory.Size()) }

// Trapped is true once a call trapped. The heap may be half-written, so
// the instance answers nothing more.
func (i *Instance) Trapped() bool { return i.trapped.Load() }

// Links says whether the module links the capability.
func (i *Instance) Links(capability string) bool {
	_, ok := i.code.caps[capability]
	return ok
}

// Capabilities are the module's, in index order.
func (i *Instance) Capabilities() []string {
	out := make([]string, len(i.code.caps))
	for id, index := range i.code.caps {
		out[index] = id
	}
	return out
}

// Documents are the kinds the capability declares.
func (i *Instance) Documents(capability string) map[string]bool {
	return i.code.documents[capability]
}

// Current makes an Instance a Source of itself.
func (i *Instance) Current() *Instance { return i }

// A Source answers the instance a call runs on now. An Instance is its
// own source. A source that replaces its instance gives each call the
// instance current when the call starts.
type Source interface {
	Current() *Instance
}

// As speaks to one capability of the instance.
func (i *Instance) As(capability string) *Host { return Speak(i, capability) }

// Speak speaks to one capability of the source's module. Each JSON request
// names it (`cap`), and each of its bulk ops carries its index in the high
// 16 bits. A kernel op (a load, evict, about) goes as it is.
func Speak(src Source, capability string) *Host {
	return &Host{src: src, cap: capability}
}

type Host struct {
	src Source
	cap string
}

func (h *Host) Capability() string { return h.cap }

// Instance is the instance a call would run on now.
func (h *Host) Instance() *Instance { return h.src.Current() }

func (h *Host) Hash() string { return h.src.Current().Hash() }

func (h *Host) Invoke(ctx context.Context, request, out any) error {
	raw, err := h.InvokeRaw(ctx, request)
	if err != nil {
		return err
	}
	if out == nil || len(raw) == 0 {
		return nil
	}
	return json.Unmarshal(raw, out)
}

// InvokeRaw returns the ok-payload's bytes untouched, for answers the host
// serves without reading.
func (h *Host) InvokeRaw(ctx context.Context, request any) (json.RawMessage, error) {
	payload, err := json.Marshal(request)
	if err != nil {
		return nil, err
	}
	payload, err = naming(payload, h.cap)
	if err != nil {
		return nil, err
	}
	raw, err := h.run(func(inst *Instance) ([]byte, error) { return inst.call(ctx, inst.invoke, payload) })
	if err != nil {
		return nil, err
	}
	var ok json.RawMessage
	return ok, decode(raw, &ok)
}

// A source that replaces its instance tells a call to start again on the
// current one, and hears of a trap so that it replaces the instance.
type watcher interface {
	Trapped(*Instance)
}

func (h *Host) run(call func(*Instance) ([]byte, error)) ([]byte, error) {
	for {
		inst := h.src.Current()
		raw, err := call(inst)
		if errors.Is(err, ErrClosed) && h.src.Current() != inst {
			continue
		}
		var trap *TrapError
		if w, ok := h.src.(watcher); ok && errors.As(err, &trap) {
			w.Trapped(inst)
		}
		return raw, err
	}
}

func naming(payload []byte, capability string) ([]byte, error) {
	if capability == "" {
		return payload, nil
	}
	if len(payload) < 2 || payload[0] != '{' {
		return nil, fmt.Errorf("a request to %s is not a JSON object", capability)
	}
	name, _ := json.Marshal(capability)
	out := append([]byte(`{"cap":`), name...)
	if string(payload) != "{}" {
		out = append(out, ',')
	}
	return append(out, payload[1:]...), nil
}

const kernelOps = 256

func (h *Host) InvokeBytes(ctx context.Context, op uint32, payload []byte, out any) error {
	raw, err := h.run(func(inst *Instance) ([]byte, error) {
		code := op
		if op < kernelOps && h.cap != "" {
			index, ok := inst.code.caps[h.cap]
			if !ok {
				return nil, fmt.Errorf("the module links no capability %q", h.cap)
			}
			code |= index << 16
		}
		return inst.call(ctx, inst.invokeBytes, payload, uint64(code))
	})
	if err != nil {
		return err
	}
	return decode(raw, out)
}

// InvokeBytes runs a bulk op as it is numbered.
func (i *Instance) InvokeBytes(ctx context.Context, op uint32, payload []byte, out any) error {
	raw, err := i.call(ctx, i.invokeBytes, payload, uint64(op))
	if err != nil {
		return err
	}
	return decode(raw, out)
}

// A TrapError is a call that failed inside the module, not an answer.
type TrapError struct{ Err error }

func (e *TrapError) Error() string { return "the module trapped: " + e.Err.Error() }
func (e *TrapError) Unwrap() error { return e.Err }

var ErrClosed = errors.New("the instance is closed")

// The whole alloc and free sequence holds the lock, because interleaved
// calls would race on the module's heap.
func (i *Instance) call(ctx context.Context, fn api.Function, payload []byte, extra ...uint64) ([]byte, error) {
	i.mu.Lock()
	defer i.mu.Unlock()
	if i.closed {
		return nil, ErrClosed
	}
	if i.trapped.Load() {
		return nil, &TrapError{errors.New("an earlier call trapped")}
	}
	trap := func(err error) error {
		i.trapped.Store(true)
		return &TrapError{err}
	}
	ptr, err := i.alloc.Call(ctx, uint64(len(payload)))
	if err != nil {
		return nil, trap(fmt.Errorf("alloc: %w", err))
	}
	in := uint32(ptr[0])
	if !i.memory.Write(in, payload) {
		return nil, fmt.Errorf("memory write at %d failed", in)
	}
	args := append(extra, uint64(in), uint64(len(payload)))
	out, err := fn.Call(ctx, args...)
	if err != nil {
		return nil, trap(err)
	}
	result := uint32(out[0])
	length, ok := i.memory.ReadUint32Le(result)
	if !ok {
		return nil, fmt.Errorf("result length read at %d failed", result)
	}
	body, ok := i.memory.Read(result+4, length)
	if !ok {
		return nil, fmt.Errorf("result read at %d (%d bytes) failed", result+4, length)
	}
	// Read returns a view into linear memory. The frees below let the
	// Rust allocator overwrite that region, so copy the payload first.
	resultCopy := make([]byte, len(body))
	copy(resultCopy, body)
	if _, err := i.free.Call(ctx, uint64(result), uint64(length+4)); err != nil {
		return nil, trap(fmt.Errorf("free result: %w", err))
	}
	if _, err := i.free.Call(ctx, uint64(in), uint64(len(payload))); err != nil {
		return nil, trap(fmt.Errorf("free input: %w", err))
	}
	return resultCopy, nil
}

// Fault makes a call that reads past the end of linear memory, so the
// module traps. Tests use it to stand for any trap.
func (i *Instance) Fault(ctx context.Context) error {
	i.mu.Lock()
	defer i.mu.Unlock()
	end := uint64(i.memory.Size())
	if _, err := i.invoke.Call(ctx, end-8, 64); err != nil {
		i.trapped.Store(true)
		return &TrapError{err}
	}
	return errors.New("the call did not trap")
}

func decode(raw []byte, out any) error {
	var outcome struct {
		OK    json.RawMessage `json:"ok"`
		Error *ModuleError    `json:"error"`
	}
	if err := json.Unmarshal(raw, &outcome); err != nil {
		return fmt.Errorf("unparseable outcome: %w", err)
	}
	if outcome.Error != nil {
		return &Refused{*outcome.Error}
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
