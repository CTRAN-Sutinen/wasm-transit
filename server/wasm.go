package main

// The capability modules this server hosts, over the wazero host in
// package wasmhost (the module ABI lives there).
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
// (wasmhost), different modules at the same time; an instance is cheap
// (well under a millisecond), so a fresh one per import or publish costs
// only its memory. Only package wasmhost touches the ABI; everything
// above it speaks typed Go values.

import (
	"context"

	"rustwasm/server/wasmhost"
)

// WasmHost is one module instance (wasmhost.Host).
type WasmHost = wasmhost.Host

// NewWasmHost loads a module and instantiates it (wasmhost.New).
func NewWasmHost(ctx context.Context, artifactPath string) (*WasmHost, error) {
	return wasmhost.New(ctx, artifactPath)
}

// The kernel's loads this host calls on transit_invoke_bytes.
const (
	opLoadNetwork = wasmhost.OpLoadNetwork
	opLoadStreets = wasmhost.OpLoadStreets
	opLoadFeed    = wasmhost.OpLoadFeed
)
