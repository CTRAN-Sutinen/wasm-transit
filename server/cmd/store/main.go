// Command store runs the sync store's offline tools (docs/FORMATS.md;
// server/store), with no server. Only verify runs a module: cap-sync,
// to fold the disruptions log's checkpoints.
//
//	store verify [-sync M] <url>  read the whole store; exit 1 on the first break.
//	                              Each log's checkpoints are compared with the
//	                              fold of their entries, by the module that owns
//	                              the log (cap-sync, -sync M)
//	store gc [-grace D] <url>     collect orphan blobs (D-117), with the
//	                              collector's own delete-capable credentials
//	store copy <dir> <url>        carry a directory store an older build
//	                              wrote into a bucket, then verify it
//	store revert [-writer W] <url> <pointer> <hash>
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
		sync := fs.String("sync", "../web/pkg/cap_sync.wasm", "the sync module (cap-sync), which folds the disruptions log's checkpoints")
		fs.Parse(args)
		need(fs.Args(), 1)
		report, err = withBackend(ctx, fs.Arg(0), func(b store.Backend) (any, error) {
			return store.VerifyFolding(ctx, b, folder(ctx, *sync))
		})
	case "gc":
		fs := flag.NewFlagSet("gc", flag.ExitOnError)
		grace := fs.Duration("grace", store.OrphanGrace, "collect only orphans older than this")
		fs.Parse(args)
		need(fs.Args(), 1)
		report, err = withBackend(ctx, fs.Arg(0), func(b store.Backend) (any, error) {
			return store.New(b).GC(int64(*grace / time.Millisecond))
		})
	case "copy":
		need(args, 2)
		report, err = withBackend(ctx, args[1], func(b store.Backend) (any, error) { return store.CopyDir(ctx, args[0], b) })
	case "revert":
		fs := flag.NewFlagSet("revert", flag.ExitOnError)
		writer := fs.String("writer", "revert", "the writer the commit names (the operator)")
		fs.Parse(args)
		need(fs.Args(), 3)
		report, err = withBackend(ctx, fs.Arg(0), func(b store.Backend) (any, error) {
			return store.Revert(store.New(b), fs.Arg(1), fs.Arg(2), *writer, time.Now().UnixMilli())
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

// folder answers each log's fold through the module that owns its kinds:
// the disruptions log through cap-sync's fold_disruptions. The module
// loads on the first checkpoint, so a store with none needs no module.
func folder(ctx context.Context, syncPath string) store.LogFolder {
	var host *wasmhost.Host
	return func(log string, checkpoint []byte, entries []store.StoredEntry) ([]byte, error) {
		if log != "disruptions" {
			return nil, fmt.Errorf("no module folds the %q log", log)
		}
		if host == nil {
			h, err := wasmhost.New(ctx, syncPath)
			if err != nil {
				return nil, fmt.Errorf("the sync module (-sync %s): %w", syncPath, err)
			}
			host = h
		}
		req := map[string]any{"op": "fold_disruptions", "entries": entries}
		if checkpoint != nil {
			req["checkpoint"] = json.RawMessage(checkpoint)
		}
		var ans struct {
			Checkpoint string `json:"checkpoint"`
		}
		if err := host.Invoke(ctx, req, &ans); err != nil {
			return nil, err
		}
		return []byte(ans.Checkpoint), nil
	}
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
	fmt.Fprintln(os.Stderr, "usage: store verify [-sync cap_sync.wasm] <url> | store gc [-grace 168h] <url> | store copy <dir> <url> | store revert [-writer W] <url> <pointer> <hash>")
	os.Exit(2)
}
