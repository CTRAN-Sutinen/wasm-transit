package store

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"mime/multipart"
	"net/http"
	"net/textproto"
	"net/url"
	"os"
	"strings"
	"time"
)

// GCS speaks the Cloud Storage JSON API on the standard library. The
// conditional create is an upload with ifGenerationMatch=0.
type GCS struct {
	httpc  *http.Client
	auth   *gcpTokenCache // nil for the emulator
	base   string         // https://storage.googleapis.com, or the emulator
	bucket string
	prefix string // "" or "<prefix>/"
}

// STORAGE_EMULATOR_HOST names a local emulator, which takes no
// credentials.
func NewGCS(_ context.Context, bucket, prefix string) (*GCS, error) {
	g := &GCS{httpc: &http.Client{Timeout: 5 * time.Minute}, base: "https://storage.googleapis.com", bucket: bucket}
	if host := os.Getenv("STORAGE_EMULATOR_HOST"); host != "" {
		if !strings.Contains(host, "://") {
			host = "http://" + host
		}
		g.base = strings.TrimRight(host, "/")
	} else {
		src, err := gcpSourceFromEnv(&http.Client{Timeout: 30 * time.Second}, os.Getenv)
		if err != nil {
			return nil, fmt.Errorf("gcs: %w", err)
		}
		g.auth = &gcpTokenCache{src: src, now: time.Now}
	}
	if prefix != "" {
		prefix += "/"
	}
	g.prefix = prefix
	return g, nil
}

type gcsError struct {
	op     string
	status int
	msg    string
}

func (e *gcsError) Error() string { return fmt.Sprintf("gcs %s: HTTP %d: %s", e.op, e.status, e.msg) }

func (e *gcsError) HTTPStatusCode() int { return e.status }

func gcsCode(err error) int {
	if e, ok := err.(*gcsError); ok {
		return e.status
	}
	return 0
}

// do sends one request; path is after the base, already escaped.
func (g *GCS) do(ctx context.Context, op, method, path string, q url.Values, hdr http.Header, body []byte) ([]byte, http.Header, error) {
	u := g.base + path
	if len(q) > 0 {
		u += "?" + q.Encode()
	}
	req, err := http.NewRequestWithContext(ctx, method, u, bytes.NewReader(body))
	if err != nil {
		return nil, nil, err
	}
	if body == nil {
		req.Body = http.NoBody
	}
	for k, vs := range hdr {
		req.Header[k] = vs
	}
	if g.auth != nil {
		tok, err := g.auth.get(ctx)
		if err != nil {
			return nil, nil, err
		}
		req.Header.Set("Authorization", "Bearer "+tok)
	}
	resp, err := g.httpc.Do(req)
	if err != nil {
		return nil, nil, err
	}
	defer resp.Body.Close()
	data, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, nil, err
	}
	if resp.StatusCode/100 != 2 {
		var e struct {
			Error struct{ Message string } `json:"error"`
		}
		msg := strings.TrimSpace(string(data))
		if json.Unmarshal(data, &e) == nil && e.Error.Message != "" {
			msg = e.Error.Message
		}
		return nil, nil, &gcsError{op: op, status: resp.StatusCode, msg: msg}
	}
	return data, resp.Header, nil
}

func (g *GCS) objPath(key string) string {
	return "/storage/v1/b/" + awsEscape(g.bucket, false) + "/o/" + awsEscape(g.prefix+key, false)
}

type gcsObject struct {
	Name       string            `json:"name"`
	Generation string            `json:"generation"`
	Updated    time.Time         `json:"updated"`
	Metadata   map[string]string `json:"metadata"`
}

func (g *GCS) attrs(ctx context.Context, key, generation string) (gcsObject, error) {
	var q url.Values
	if generation != "" {
		q = url.Values{"generation": {generation}}
	}
	data, _, err := g.do(ctx, "objects.get", "GET", g.objPath(key), q, nil, nil)
	if gcsCode(err) == 404 {
		return gcsObject{}, ErrNotFound
	}
	if err != nil {
		return gcsObject{}, err
	}
	var o gcsObject
	if err := json.Unmarshal(data, &o); err != nil {
		return gcsObject{}, fmt.Errorf("gcs objects.get: %w", err)
	}
	return o, nil
}

func (g *GCS) Get(ctx context.Context, key string) ([]byte, map[string]string, error) {
	data, h, err := g.do(ctx, "objects.get media", "GET", g.objPath(key), url.Values{"alt": {"media"}}, nil, nil)
	if gcsCode(err) == 404 {
		return nil, nil, ErrNotFound
	}
	if err != nil {
		return nil, nil, err
	}
	// The download carries no metadata; read it at the generation just
	// read, so the two halves are one object.
	o, err := g.attrs(ctx, key, h.Get("X-Goog-Generation"))
	if err != nil {
		return nil, nil, err
	}
	meta := map[string]string{}
	for k, v := range o.Metadata {
		meta[strings.ToLower(k)] = v
	}
	return data, meta, nil
}

func (g *GCS) Stat(ctx context.Context, key string) (ObjectInfo, error) {
	o, err := g.attrs(ctx, key, "")
	if err != nil {
		return ObjectInfo{}, err
	}
	return ObjectInfo{Key: key, Modified: o.Updated}, nil
}

// Create is one multipart upload: the object lands whole or not at all.
func (g *GCS) Create(ctx context.Context, key string, data []byte, meta map[string]string) error {
	head, err := json.Marshal(map[string]any{"name": g.prefix + key, "metadata": meta})
	if err != nil {
		return err
	}
	var body bytes.Buffer
	mw := multipart.NewWriter(&body)
	p, _ := mw.CreatePart(textproto.MIMEHeader{"Content-Type": {"application/json; charset=UTF-8"}})
	p.Write(head)
	p, _ = mw.CreatePart(textproto.MIMEHeader{"Content-Type": {"application/octet-stream"}})
	p.Write(data)
	mw.Close()
	q := url.Values{"uploadType": {"multipart"}, "ifGenerationMatch": {"0"}}
	hdr := http.Header{"Content-Type": {"multipart/related; boundary=" + mw.Boundary()}}
	_, _, err = g.do(ctx, "objects.insert", "POST", "/upload/storage/v1/b/"+awsEscape(g.bucket, false)+"/o", q, hdr, body.Bytes())
	if gcsCode(err) == http.StatusPreconditionFailed {
		return ErrExists
	}
	return err
}

func (g *GCS) List(ctx context.Context, prefix, startAfter string) ([]ObjectInfo, error) {
	q := url.Values{"prefix": {g.prefix + prefix}, "fields": {"items(name,updated),nextPageToken"}}
	if startAfter != "" {
		q.Set("startOffset", g.prefix+startAfter) // inclusive: filtered below
	}
	var out []ObjectInfo
	for {
		data, _, err := g.do(ctx, "objects.list", "GET", "/storage/v1/b/"+awsEscape(g.bucket, false)+"/o", q, nil, nil)
		if err != nil {
			return nil, err
		}
		var page struct {
			Items         []gcsObject `json:"items"`
			NextPageToken string      `json:"nextPageToken"`
		}
		if err := json.Unmarshal(data, &page); err != nil {
			return nil, fmt.Errorf("gcs objects.list: %w", err)
		}
		for _, o := range page.Items {
			key := strings.TrimPrefix(o.Name, g.prefix)
			if key <= startAfter {
				continue
			}
			out = append(out, ObjectInfo{Key: key, Modified: o.Updated})
		}
		if page.NextPageToken == "" {
			return out, nil
		}
		q.Set("pageToken", page.NextPageToken)
	}
}

func (g *GCS) Delete(ctx context.Context, key string) error {
	_, _, err := g.do(ctx, "objects.delete", "DELETE", g.objPath(key), nil, nil, nil)
	if gcsCode(err) == 404 {
		return nil
	}
	return err
}
