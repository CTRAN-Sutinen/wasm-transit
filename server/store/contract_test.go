package store

// The backend contract (docs/units/store-durability.md, "The portable
// contract"): every backend passes the same laws. mem:// runs in
// `just test`; a real bucket runs only when STORE_TEST_URL names one
// (s3://bucket/prefix?endpoint=…, credentials from the environment) —
// never in `just test`'s required path. Each test gets its own fresh
// prefix in that bucket.

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"os"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/Azure/azure-sdk-for-go/sdk/storage/azblob/bloberror"
	"github.com/aws/aws-sdk-go-v2/service/s3"
)

type testBackend struct {
	name string
	mk   func(t *testing.T) Backend
}

// backends answers mem:// always, and the bucket STORE_TEST_URL names
// when it is set — each call a fresh, empty prefix.
func backends(t *testing.T) []testBackend {
	out := []testBackend{{"mem", func(*testing.T) Backend { return NewMemory() }}}
	if url := os.Getenv("STORE_TEST_URL"); url != "" {
		out = append(out, testBackend{"bucket", func(t *testing.T) Backend {
			t.Helper()
			var nonce [6]byte
			rand.Read(nonce[:])
			b, err := OpenBackend(context.Background(), withSubPrefix(url, "test-"+hex.EncodeToString(nonce[:])))
			if err != nil {
				t.Fatalf("open %s: %v", url, err)
			}
			ensureBucket(t, b)
			return b
		}})
	}
	return out
}

// ensureBucket creates the bucket (container) a local emulator starts
// without — tests only: the adapters never create one, a deployment's
// bucket is provisioned with its retention settings.
func ensureBucket(t *testing.T, b Backend) {
	t.Helper()
	ensureOnce.Lock()
	defer ensureOnce.Unlock()
	ctx := context.Background()
	var err error
	switch b := b.(type) {
	case *S3:
		if ensured[b.bucket] {
			return
		}
		_, err = b.c.CreateBucket(ctx, &s3.CreateBucketInput{Bucket: &b.bucket})
		if status(err) == 409 {
			err = nil
		}
		ensured[b.bucket] = true
	case *Azure:
		if ensured[b.container] {
			return
		}
		_, err = b.c.CreateContainer(ctx, b.container, nil)
		if bloberror.HasCode(err, bloberror.ContainerAlreadyExists) {
			err = nil
		}
		ensured[b.container] = true
	case *GCS:
		if ensured[b.b.BucketName()] {
			return
		}
		err = b.b.Create(ctx, "transit-test", nil)
		if gcsCode(err) == 409 {
			err = nil
		}
		ensured[b.b.BucketName()] = true
	}
	if err != nil {
		t.Fatalf("ensure bucket: %v", err)
	}
}

var (
	ensureOnce sync.Mutex
	ensured    = map[string]bool{}
)

// withSubPrefix appends one path segment to a store URL's prefix,
// keeping its query.
func withSubPrefix(url, seg string) string {
	base, query, _ := strings.Cut(url, "?")
	base = strings.TrimSuffix(base, "/") + "/" + seg
	if query != "" {
		return base + "?" + query
	}
	return base
}

func eachBackend(t *testing.T, f func(t *testing.T, b Backend)) {
	for _, tb := range backends(t) {
		t.Run(tb.name, func(t *testing.T) { f(t, tb.mk(t)) })
	}
}

func TestContractCreateIsConditionalAndWhole(t *testing.T) {
	eachBackend(t, func(t *testing.T, b Backend) {
		ctx := context.Background()
		meta := map[string]string{"envelope": `{"kind":"manifest","schema_version":1,"produced_by":"server"}`}
		if err := b.Create(ctx, "k/one", []byte("first"), meta); err != nil {
			t.Fatalf("create: %v", err)
		}
		// The second create of a present key fails and leaves the object.
		if err := b.Create(ctx, "k/one", []byte("second"), nil); !errors.Is(err, ErrExists) {
			t.Fatalf("a create over a present key must be ErrExists: %v", err)
		}
		data, got, err := b.Get(ctx, "k/one")
		if err != nil || string(data) != "first" {
			t.Fatalf("read after write: %q %v", data, err)
		}
		if got["envelope"] != meta["envelope"] {
			t.Fatalf("metadata round trip: %q", got)
		}
		if _, _, err := b.Get(ctx, "k/absent"); !errors.Is(err, ErrNotFound) {
			t.Fatalf("an absent key must be ErrNotFound: %v", err)
		}
		// An empty object is an object.
		if err := b.Create(ctx, "k/empty", nil, nil); err != nil {
			t.Fatal(err)
		}
		if data, _, err := b.Get(ctx, "k/empty"); err != nil || len(data) != 0 {
			t.Fatalf("empty object: %q %v", data, err)
		}
	})
}

func TestContractRacingCreatesHaveOneWinner(t *testing.T) {
	eachBackend(t, func(t *testing.T, b Backend) {
		ctx := context.Background()
		const racers = 8
		var wg sync.WaitGroup
		wins := make(chan int, racers)
		for i := 0; i < racers; i++ {
			wg.Add(1)
			go func(i int) {
				defer wg.Done()
				err := b.Create(ctx, "race/00000000000000000001.json", []byte{byte('a' + i)}, nil)
				switch {
				case err == nil:
					wins <- i
				case !errors.Is(err, ErrExists):
					t.Errorf("racer %d: %v", i, err)
				}
			}(i)
		}
		wg.Wait()
		close(wins)
		var won []int
		for i := range wins {
			won = append(won, i)
		}
		if len(won) != 1 {
			t.Fatalf("exactly one racing create wins, got %v", won)
		}
		data, _, err := b.Get(ctx, "race/00000000000000000001.json")
		if err != nil || len(data) != 1 || data[0] != byte('a'+won[0]) {
			t.Fatalf("the object is the winner's bytes: %q %v", data, err)
		}
	})
}

func TestContractListIsOrderedPrefixedAndFresh(t *testing.T) {
	eachBackend(t, func(t *testing.T, b Backend) {
		ctx := context.Background()
		for _, k := range []string{"p/a/3", "p/a/1", "p/ab/1", "p/a/2", "q/1"} {
			if err := b.Create(ctx, k, []byte(k), nil); err != nil {
				t.Fatal(err)
			}
		}
		keys := func(infos []ObjectInfo) string {
			var ks []string
			for _, o := range infos {
				ks = append(ks, o.Key)
			}
			return strings.Join(ks, ",")
		}
		all, err := b.List(ctx, "p/a/", "")
		if err != nil || keys(all) != "p/a/1,p/a/2,p/a/3" {
			t.Fatalf("list after write, ascending, prefix-exact: %q %v", keys(all), err)
		}
		after, err := b.List(ctx, "p/a/", "p/a/1")
		if err != nil || keys(after) != "p/a/2,p/a/3" {
			t.Fatalf("list from a key: %q %v", keys(after), err)
		}
		none, err := b.List(ctx, "p/none/", "")
		if err != nil || len(none) != 0 {
			t.Fatalf("an empty prefix lists nothing: %v %v", none, err)
		}
		for _, o := range all {
			if o.Modified.IsZero() || time.Since(o.Modified) > time.Hour {
				t.Fatalf("a listed object carries its modified time: %+v", o)
			}
		}
	})
}

func TestContractStatAndDelete(t *testing.T) {
	eachBackend(t, func(t *testing.T, b Backend) {
		ctx := context.Background()
		if err := b.Create(ctx, "d/x", []byte("x"), nil); err != nil {
			t.Fatal(err)
		}
		info, err := b.Stat(ctx, "d/x")
		if err != nil || info.Key != "d/x" || time.Since(info.Modified) > time.Hour {
			t.Fatalf("stat: %+v %v", info, err)
		}
		if err := b.Delete(ctx, "d/x"); err != nil {
			t.Fatal(err)
		}
		if _, err := b.Stat(ctx, "d/x"); !errors.Is(err, ErrNotFound) {
			t.Fatalf("a deleted key is absent: %v", err)
		}
		if err := b.Delete(ctx, "d/x"); err != nil {
			t.Fatalf("deleting an absent key is not an error: %v", err)
		}
		// A deleted key can be created again (GC's orphan, re-put).
		if err := b.Create(ctx, "d/x", []byte("y"), nil); err != nil {
			t.Fatalf("create after delete: %v", err)
		}
	})
}
