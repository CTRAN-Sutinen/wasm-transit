// Package manifest is the deployment's root document: what each page
// reads first (docs/FORMATS.md, "The manifest"). In store mode it is the
// `manifest` pointer, and publishing is one CAS on it.
package manifest

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"reflect"
	"strings"
	"sync"
	"time"

	"rustwasm/server/httpx"
	"rustwasm/server/store"
)

// A Publisher serves /manifest. Build gathers what the manifest pins.
type Publisher struct {
	store func() *store.Store
	build func() (Manifest, error)

	mu sync.Mutex
	// key is what the stored manifest pins (KeyOf).
	key string
}

func New(st func() *store.Store, build func() (Manifest, error)) *Publisher {
	return &Publisher{store: st, build: build}
}

type Manifest struct {
	NetworkHash  string `json:"network_hash"` // the network GET /network serves
	BoundaryHash string `json:"boundary_hash,omitempty"`
	// A serverless deployment that follows the agency's GTFS URL
	// (-export with -feed-url) publishes no network. Each page fetches the
	// zip, imports it, and checks it daily.
	FeedURL     string            `json:"feed_url,omitempty"`
	FeedHash    string            `json:"feed_hash"` // "sha256:<hex>" of the GTFS zip it was imported from
	FeedVersion *string           `json:"feed_version,omitempty"`
	StreetsHash string            `json:"streets_hash,omitempty"` // the streets document GET /streets serves
	WalksHash   string            `json:"walks_hash,omitempty"`   // the walk table GET /walks serves
	WasmHash    string            `json:"wasm_hash"`              // sha256 of the module this server runs
	AuthzHash   string            `json:"authz_hash,omitempty"`   // sha256 of the module authz runs in, when auth is on
	PolicyHash  string            `json:"policy_hash,omitempty"`  // the store's policy pointer head, store mode
	Logged      map[string]string `json:"-"`
	// The seq of each log's fold that this manifest pins. A page sends it
	// with a write as read_seq.
	LogSeqs     map[string]int64 `json:"log_seqs,omitempty"`
	BuildHash   string           `json:"build_hash,omitempty"` // the build the pages are served from (the build pointer head), store mode
	GeneratedAt int64            `json:"generated_at"`
	Publisher   *string          `json:"publisher,omitempty"`
	// A page reads its clock in this zone, never the device's
	// (web/src/clock.ts). Empty while none is known.
	Timezone string `json:"timezone,omitempty"`
	// Absent when sign-in is off or the pages have no client of their own.
	Auth *Auth `json:"auth,omitempty"`
	// A service that is not listed is off (docs/architecture/web.md,
	// "External services").
	Services map[string]Service `json:"services,omitempty"`
}

func (m Manifest) MarshalJSON() ([]byte, error) {
	type plain Manifest
	raw, err := json.Marshal(plain(m))
	if err != nil || len(m.Logged) == 0 {
		return raw, err
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(raw, &fields); err != nil {
		return nil, err
	}
	for field, hash := range m.Logged {
		fields[field], _ = json.Marshal(hash)
	}
	return json.Marshal(fields)
}

func (m *Manifest) UnmarshalJSON(raw []byte) error {
	type plain Manifest
	if err := json.Unmarshal(raw, (*plain)(m)); err != nil {
		return err
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(raw, &fields); err != nil {
		return err
	}
	known := map[string]bool{}
	t := reflect.TypeOf(plain{})
	for i := 0; i < t.NumField(); i++ {
		name, _, _ := strings.Cut(t.Field(i).Tag.Get("json"), ",")
		known[name] = true
	}
	for field, v := range fields {
		var hash string
		if strings.HasSuffix(field, "_hash") && !known[field] && json.Unmarshal(v, &hash) == nil {
			if m.Logged == nil {
				m.Logged = map[string]string{}
			}
			m.Logged[field] = hash
		}
	}
	return nil
}

// Writes need a token and reads stay public.
type Auth struct {
	Issuer   string `json:"issuer"`
	ClientID string `json:"client_id"`
	Scope    string `json:"scope"`
}

type Service struct {
	Path  string                    `json:"path"`
	Query map[string]map[string]any `json:"query,omitempty"`
}

// KeyOf covers everything but the generated_at stamp.
func KeyOf(m Manifest) string {
	services, _ := json.Marshal(m.Services)
	seqs, _ := json.Marshal(m.LogSeqs)
	sum := sha256.Sum256([]byte(strings.Join([]string{string(seqs),
		m.WasmHash, m.AuthzHash, m.PolicyHash, fmt.Sprint(m.Logged), m.BuildHash, m.NetworkHash, m.BoundaryHash, m.FeedHash, orEmpty(m.FeedVersion),
		m.StreetsHash, m.WalksHash, orEmpty(m.Publisher), m.Timezone, string(services), m.FeedURL, fmt.Sprint(m.Auth),
	}, "|")))
	return hex.EncodeToString(sum[:])
}

func orEmpty(s *string) string {
	if s == nil {
		return ""
	}
	return *s
}

func (p *Publisher) HandleManifest(w http.ResponseWriter, r *http.Request) {
	manifest, err := p.build()
	if err != nil {
		httpx.Fail(w, err, http.StatusInternalServerError)
		return
	}
	if p.store() == nil {
		// The dev store serves the manifest as built. The ETag names what it
		// pins, so an open page's poll is a 304 while nothing moved.
		etag := `"` + KeyOf(manifest) + `"`
		w.Header().Set("ETag", etag)
		w.Header().Set("Cache-Control", "no-cache")
		if r.Header.Get("If-None-Match") == etag {
			w.WriteHeader(http.StatusNotModified)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(manifest)
		return
	}
	// In store mode the manifest is the root pointer. Publishing is one
	// CAS on it (docs/FORMATS.md).
	p.mu.Lock()
	key := KeyOf(manifest)
	if key != p.key {
		body, _ := json.Marshal(manifest)
		env := store.Envelope{Kind: "manifest", SchemaVersion: 1, ProducedBy: "server"}
		for attempt := 0; attempt < 5; attempt++ {
			head, found, err := p.store().Head("manifest")
			if err != nil {
				p.mu.Unlock()
				httpx.Fail(w, err, http.StatusInternalServerError)
				return
			}
			from := ""
			if found {
				from = head.Hash
			}
			_, _, err = p.store().CAS("manifest", from, body, env, "", "server", time.Now().UnixMilli())
			if errors.Is(err, store.ErrUnavailable) {
				break // serve the cached manifest below, labelled
			}
			if err != nil {
				var ce *store.ConflictError
				if errors.As(err, &ce) && attempt < 4 {
					continue
				}
				p.mu.Unlock()
				httpx.Fail(w, err, http.StatusInternalServerError)
				return
			}
			p.key = key
			break
		}
	}
	p.mu.Unlock()

	head, found, err := p.store().Head("manifest")
	if err != nil {
		httpx.Fail(w, err, http.StatusInternalServerError)
		return
	}
	if !found {
		httpx.Fail(w, store.ErrUnavailable, http.StatusInternalServerError)
		return
	}
	data, _, err := p.store().GetBlob(head.Hash)
	if err != nil {
		httpx.Fail(w, err, http.StatusInternalServerError)
		return
	}
	httpx.LabelStale(w, head)
	etag := `"` + head.Hash + `"`
	w.Header().Set("ETag", etag)
	w.Header().Set("Cache-Control", "no-cache")
	if r.Header.Get("If-None-Match") == etag {
		w.WriteHeader(http.StatusNotModified)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	w.Write(data)
}
