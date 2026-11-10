// Command store runs the sync store's offline tools (docs/FORMATS.md;
// server/store): Go only, no wasm, no server.
//
//	store verify <url>            read the whole store; exit 1 on the first break
//	store gc [-grace D] <url>     collect orphan blobs (D-117), with the
//	                              collector's own delete-capable credentials
//	store copy <dir> <url>        carry a directory store an older build
//	                              wrote into a bucket, then verify it
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
		need(args, 1)
		report, err = withBackend(ctx, args[0], func(b store.Backend) (any, error) { return store.Verify(ctx, b) })
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
	fmt.Fprintln(os.Stderr, "usage: store verify <url> | store gc [-grace 168h] <url> | store copy <dir> <url>")
	os.Exit(2)
}
