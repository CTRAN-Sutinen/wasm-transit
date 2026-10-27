package main

// The module is one instance with single-instance state (its heap,
// its linear memory). net/http serves each request on its own
// goroutine, so the host must serialize invokes — this test is what
// makes `go test -race` actually watch that: concurrent goroutines
// hammer the JSON seam against one host.

import (
	"context"
	"fmt"
	"sync"
	"testing"
)

func TestConcurrentInvokes(t *testing.T) {
	host := newTestHost(t)
	defer host.Close(context.Background())

	const workers = 8
	const rounds = 20
	var wg sync.WaitGroup
	errs := make(chan error, workers*rounds)

	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func(w int) {
			defer wg.Done()
			ctx := context.Background()
			for i := 0; i < rounds; i++ {
				// The JSON seam: pure and deterministic (the
				// capability's dispatch test pins its exact
				// answers).
				var out struct {
					Records []any `json:"records"`
				}
				if err := host.Invoke(ctx, map[string]any{
					"op": "sync_edits", "client": []any{}, "server": []any{}, "now_ms": 1000,
				}, &out); err != nil {
					errs <- fmt.Errorf("worker %d: sync_edits: %w", w, err)
					return
				}
				if len(out.Records) != 0 {
					errs <- fmt.Errorf("worker %d: sync_edits returned %d records, want 0", w, len(out.Records))
					return
				}
			}
		}(w)
	}

	wg.Wait()
	close(errs)
	for err := range errs {
		t.Fatal(err)
	}
}
