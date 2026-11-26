package main

// Only package wasmhost touches the module ABI. Everything above it
// speaks typed Go values. The server runs one module (`-wasm`) as
// the instances docs/architecture/server.md names: the long-lived one,
// authz's own, and a fresh one for each import or publish.

import (
	"context"
	_ "embed"
	"encoding/json"

	"rustwasm/server/wasmhost"
)

type WasmHost = wasmhost.Host
type Instance = wasmhost.Instance

const defaultWasm = "../web/pkg/server.wasm"

// NewWasmHost compiles the module once for each process, and makes a new
// instance of it each call.
func NewWasmHost(ctx context.Context, path string) (*Instance, error) {
	return wasmhost.New(ctx, path)
}

// The server's one module. Tests make a server around one instance
// (`wasm`), and every other instance comes from it.
func (s *server) longLived() *wasmhost.Held {
	s.longOnce.Do(func() {
		if s.long == nil {
			s.long = wasmhost.NewHeld("long-lived", s.wasm.Instance())
		}
	})
	return s.long
}

func (s *server) as(capability string) *WasmHost { return s.longLived().As(capability) }

// withModule runs `use` as the capability on the long-lived instance.
func (s *server) withModule(ctx context.Context, capability string, use func(*WasmHost) error) error {
	return s.longLived().Sequence(ctx, func(inst *Instance) error { return use(inst.As(capability)) })
}

// fresh is an instance of its own, for an import or a publish, so the
// memory it used goes with it. The caller closes it.
func (s *server) fresh(ctx context.Context) (*Instance, error) {
	return s.longLived().Current().Fresh(ctx)
}

// The capabilities the server calls by name. gen-assemblies links them
// into the server's module, with each one the kind list names.
//
//go:embed capabilities.json
var ownCapabilities []byte

func unlinked(inst *Instance) []string {
	var own []string
	json.Unmarshal(ownCapabilities, &own)
	var missing []string
	for _, id := range own {
		if !inst.Links(id) {
			missing = append(missing, id)
		}
	}
	return missing
}

// unhosted are the capabilities the kind table names that the module
// does not link.
func (s *server) unhosted() []string {
	t, err := s.kindTable()
	if err != nil {
		return nil
	}
	inst := s.longLived().Current()
	seen := map[string]bool{}
	var missing []string
	for _, e := range t.Kinds {
		for _, id := range []string{e.Module, e.Receive} {
			if id != "" && !seen[id] && !inst.Links(id) {
				missing = append(missing, id)
			}
			seen[id] = true
		}
	}
	return missing
}
