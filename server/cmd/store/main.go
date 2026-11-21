// Command store runs the sync store's offline tools (docs/FORMATS.md;
// server/store), with no server. Only verify runs a module: cap-log, to
// fold the logs' checkpoints.
//
//	store verify [-log M] <url>   read the whole store; exit 1 on the first break.
//	                              Each log's checkpoints are compared with the
//	                              fold of their entries, by cap-log (-log)
//	store gc [-grace D] <url>     collect orphan blobs (D-117), with the
//	                              collector's own delete-capable credentials
//	store dump <url> <dir>        write every object to a folder, to read the
//	                              store with no object storage tools: each at
//	                              <dir>/<key>, a blob's envelope beside it
//	                              (<key>.meta.json); read only
//	store copy <dir> <url>        carry a directory store an older build
//	                              wrote into a bucket, then verify it
//	store revert [-writer W] [-log cap_log.wasm] [-kinds kinds.json] <url> <pointer> <hash>
//	                              the emergency procedure: move a pointer
//	                              back to a head its log holds, as a new
//	                              logged commit, and list the commits the
//	                              move dropped (docs/FORMATS.md, "Rolling
//	                              a version back")
//
// <url> is a store URL: s3://bucket/prefix[?endpoint=URL&region=R],
// azblob://container/prefix, or gs://bucket/prefix, with each
// provider's usual credentials (DEPLOYMENTS-SERVER.md, "Prepare the
// bucket"). Each prints its report
// as JSON on stdout.
package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"time"

	"rustwasm/server/store"
	"rustwasm/server/wasmhost"
)

func main() {
	if len(os.Args) < 2 {
		usage()
	}
	ctx := context.Background()
	var report any
	var err error
	switch cmd, args := os.Args[1], os.Args[2:]; cmd {
	case "verify":
		fs := flag.NewFlagSet("verify", flag.ExitOnError)
		logm := fs.String("log", "../web/pkg/cap_log.wasm", "the log module (cap-log), which folds each log's checkpoints")
		kinds := fs.String("kinds", "../web/pkg/kinds.json", "the kind table the existence check judges with (D-233): each broken reference of each log's fold at its head is reported, with the entry that broke it; empty: no check")
		fs.Parse(args)
		need(fs.Args(), 1)
		report, err = withBackend(ctx, fs.Arg(0), func(b store.Backend) (any, error) {
			m := &logModule{ctx: ctx, path: *logm}
			if *kinds == "" {
				return store.VerifyFolding(ctx, b, m.fold)
			}
			table, err := os.ReadFile(*kinds)
			if err != nil {
				return nil, fmt.Errorf("-kinds: %w", err)
			}
			return store.VerifyChecking(ctx, b, m.fold, m.references(table))
		})
	case "gc":
		fs := flag.NewFlagSet("gc", flag.ExitOnError)
		grace := fs.Duration("grace", store.OrphanGrace, "collect only orphans older than this")
		fs.Parse(args)
		need(fs.Args(), 1)
		report, err = withBackend(ctx, fs.Arg(0), func(b store.Backend) (any, error) {
			return store.New(b).GC(int64(*grace / time.Millisecond))
		})
	case "dump":
		need(args, 2)
		report, err = withBackend(ctx, args[0], func(b store.Backend) (any, error) { return store.Dump(ctx, b, args[1]) })
	case "copy":
		need(args, 2)
		report, err = withBackend(ctx, args[1], func(b store.Backend) (any, error) { return store.CopyDir(ctx, args[0], b) })
	case "revert":
		fs := flag.NewFlagSet("revert", flag.ExitOnError)
		writer := fs.String("writer", "revert", "the writer the commit names (the operator)")
		logm := fs.String("log", "../web/pkg/cap_log.wasm", "the log module (cap-log), which finds the records that name a dropped commit")
		kinds := fs.String("kinds", "../web/pkg/kinds.json", "the kind table whose references name the dropped commits (D-233); empty: none are listed")
		fs.Parse(args)
		need(fs.Args(), 3)
		report, err = withBackend(ctx, fs.Arg(0), func(b store.Backend) (any, error) {
			r, err := store.Revert(store.New(b), fs.Arg(1), fs.Arg(2), *writer, time.Now().UnixMilli())
			if err != nil || len(r.Dropped) == 0 || *kinds == "" {
				return r, err
			}
			table, err := os.ReadFile(*kinds)
			if err != nil {
				return r, fmt.Errorf("the revert is done; -kinds: %w", err)
			}
			m := &logModule{ctx: ctx, path: *logm}
			if r.References, err = m.naming(b, table, r.Dropped); err != nil {
				return r, fmt.Errorf("the revert is done; the references into what it dropped: %w", err)
			}
			return r, nil
		})
	default:
		usage()
	}
	if report != nil {
		out, _ := json.MarshalIndent(report, "", "  ")
		fmt.Println(string(out))
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

// logModule is cap-log (D-197), loaded on first use, so a store with no
// checkpoint and no existence check needs no module.
type logModule struct {
	ctx  context.Context
	path string
	host *wasmhost.Host
}

func (m *logModule) load(why string) (*wasmhost.Host, error) {
	if m.host == nil {
		h, err := wasmhost.New(m.ctx, m.path)
		if err != nil {
			return nil, fmt.Errorf("the module that %s (-log %s): %w", why, m.path, err)
		}
		m.host = h
	}
	return m.host, nil
}

// fold answers a log's fold through cap-log.
func (m *logModule) fold(log string, checkpoint []byte, entries []store.StoredEntry) ([]byte, error) {
	host, err := m.load("folds the " + log + " log")
	if err != nil {
		return nil, err
	}
	req := map[string]any{"op": "fold", "log": log, "entries": entries}
	if checkpoint != nil {
		req["checkpoint"] = json.RawMessage(checkpoint)
	}
	var ans struct {
		Checkpoint string `json:"checkpoint"`
	}
	if err := host.Invoke(m.ctx, req, &ans); err != nil {
		return nil, err
	}
	return []byte(ans.Checkpoint), nil
}

// folder answers each log's fold through cap-log.
func folder(ctx context.Context, logPath string) store.LogFolder {
	m := &logModule{ctx: ctx, path: logPath}
	return m.fold
}

// references is the existence check through cap-log's references op,
// over the kind table's bytes (D-233).
func (m *logModule) references(table []byte) store.ReferenceChecker {
	return func(heads map[string][]byte, entries map[string][]store.StoredEntry) (*store.ReferenceFindings, error) {
		kinds, err := tableKinds(table)
		if err != nil {
			return nil, err
		}
		folds, err := recordsOf(heads)
		if err != nil {
			return nil, err
		}
		host, err := m.load("checks the references")
		if err != nil {
			return nil, err
		}
		var ans struct {
			Broken   []json.RawMessage `json:"broken"`
			Unjudged []json.RawMessage `json:"unjudged"`
		}
		if err := host.Invoke(m.ctx, map[string]any{"op": "references", "kinds": kinds, "folds": folds, "entries": entries}, &ans); err != nil {
			return nil, err
		}
		if ans.Broken == nil {
			ans.Broken = []json.RawMessage{}
		}
		return &store.ReferenceFindings{KindTable: store.HashOf(table), Module: "cap-log@" + host.Hash(), Broken: ans.Broken, Unjudged: ans.Unjudged}, nil
	}
}

// tableKinds are a kind table's kinds, as cap-log reads them.
func tableKinds(table []byte) (json.RawMessage, error) {
	var t struct {
		Kinds json.RawMessage `json:"kinds"`
	}
	if err := json.Unmarshal(table, &t); err != nil || t.Kinds == nil {
		return nil, fmt.Errorf("-kinds: not a kind table: %v", err)
	}
	return t.Kinds, nil
}

// recordsOf are a fold's records (a checkpoint's), by log.
func recordsOf(heads map[string][]byte) (map[string]json.RawMessage, error) {
	folds := map[string]json.RawMessage{}
	for log, head := range heads {
		var c struct {
			Records json.RawMessage `json:"records"`
		}
		if err := json.Unmarshal(head, &c); err != nil {
			return nil, fmt.Errorf("the %s log's fold: %w", log, err)
		}
		folds[log] = c.Records
	}
	return folds, nil
}

// naming are the records of every log's head that name one of the
// dropped commits' documents by hash (cap-log naming, D-233).
func (m *logModule) naming(b store.Backend, table []byte, dropped []store.LogEntry) ([]json.RawMessage, error) {
	kinds, err := tableKinds(table)
	if err != nil {
		return nil, err
	}
	heads, _, err := store.HeadFolds(m.ctx, b, m.fold)
	if err != nil {
		return nil, err
	}
	folds, err := recordsOf(heads)
	if err != nil {
		return nil, err
	}
	hashes := []string{}
	for _, d := range dropped {
		hashes = append(hashes, d.To)
	}
	host, err := m.load("finds the references")
	if err != nil {
		return nil, err
	}
	var ans struct {
		References []json.RawMessage `json:"references"`
	}
	if err := host.Invoke(m.ctx, map[string]any{"op": "naming", "kinds": kinds, "folds": folds, "hashes": hashes}, &ans); err != nil {
		return nil, err
	}
	if ans.References == nil {
		ans.References = []json.RawMessage{}
	}
	return ans.References, nil
}

func withBackend(ctx context.Context, url string, f func(store.Backend) (any, error)) (any, error) {
	b, err := store.OpenBackend(ctx, url)
	if err != nil {
		return nil, err
	}
	return f(b)
}

func need(args []string, n int) {
	if len(args) != n {
		usage()
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "usage: store verify [-log cap_log.wasm] [-kinds kinds.json] <url> | store gc [-grace 168h] <url> | store dump <url> <dir> | store copy <dir> <url> | store revert [-writer W] [-log cap_log.wasm] [-kinds kinds.json] <url> <pointer> <hash>")
	os.Exit(2)
}
