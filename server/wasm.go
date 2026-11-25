package main

// Only package wasmhost touches the module ABI. Everything above it
// speaks typed Go values. docs/architecture/server.md lists the hosted
// modules and how long each instance lives.

import (
	"context"

	"rustwasm/server/wasmhost"
)

type WasmHost = wasmhost.Host

func NewWasmHost(ctx context.Context, artifactPath string) (*WasmHost, error) {
	return wasmhost.New(ctx, artifactPath)
}

const (
	opLoadNetwork = wasmhost.OpLoadNetwork
	opLoadStreets = wasmhost.OpLoadStreets
	opLoadFeed    = wasmhost.OpLoadFeed
)
