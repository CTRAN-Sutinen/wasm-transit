package store

import (
	"bytes"
	"context"
	"encoding/xml"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strings"
	"time"
)

// S3 speaks the S3 REST API on the standard library. The conditional
// create is PutObject with If-None-Match: *.
type S3 struct {
	httpc     *http.Client
	creds     *awsCredCache
	base      *url.URL // the endpoint; the bucket is its host's first label or its path's first segment
	pathStyle bool
	region    string
	bucket    string
	prefix    string // "" or "<prefix>/"
	now       func() time.Time
}

// A non-empty endpoint names an S3-compatible server and uses path-style
// addressing. region defaults to AWS_REGION, then AWS_DEFAULT_REGION,
// then us-east-1.
func NewS3(_ context.Context, bucket, prefix, endpoint, region string) (*S3, error) {
	if region == "" {
		region = os.Getenv("AWS_REGION")
	}
	if region == "" {
		region = os.Getenv("AWS_DEFAULT_REGION")
	}
	if region == "" {
		region = "us-east-1"
	}
	b := &S3{
		httpc:  &http.Client{Timeout: 5 * time.Minute},
		region: region,
		bucket: bucket,
		now:    time.Now,
	}
	if endpoint != "" {
		u, err := url.Parse(strings.TrimRight(endpoint, "/"))
		if err != nil || u.Host == "" {
			return nil, fmt.Errorf("s3 endpoint %q: not a URL", endpoint)
		}
		b.base, b.pathStyle = u, true
	} else {
		b.base = &url.URL{Scheme: "https", Host: bucket + ".s3." + region + ".amazonaws.com"}
	}
	b.creds = &awsCredCache{src: defaultAWSEnv().source(region), now: time.Now}
	if prefix != "" {
		prefix += "/"
	}
	b.prefix = prefix
	return b, nil
}

// s3Error is a non-2xx answer.
type s3Error struct {
	op     string
	status int
	code   string
	msg    string
}

func (e *s3Error) Error() string {
	return fmt.Sprintf("s3 %s: HTTP %d %s %s", e.op, e.status, e.code, e.msg)
}

func (e *s3Error) HTTPStatusCode() int { return e.status }

// do sends one signed request. key "" with query names the bucket.
func (b *S3) do(ctx context.Context, method, key string, q url.Values, hdr http.Header, body []byte) (*http.Response, error) {
	u := *b.base
	p := "/" + key
	if b.pathStyle {
		p = strings.TrimRight(u.Path, "/") + "/" + b.bucket + p
		if key == "" {
			p = strings.TrimSuffix(p, "/")
		}
	}
	u.Path = p
	u.RawPath = awsEscapePath(p)
	u.RawQuery = awsCanonicalQuery(q)
	req, err := http.NewRequestWithContext(ctx, method, u.String(), bytes.NewReader(body))
	if err != nil {
		return nil, err
	}
	req.ContentLength = int64(len(body))
	if body == nil {
		req.Body = http.NoBody
	}
	for k, vs := range hdr {
		req.Header[k] = vs
	}
	c, err := b.creds.get(ctx)
	if err != nil {
		return nil, err
	}
	hash := emptySHA256
	if len(body) > 0 {
		hash = sha256Hex(body)
	}
	signV4(req, hash, c, b.region, "s3", b.now())
	return b.httpc.Do(req)
}

// check answers the body of a 2xx, and an *s3Error otherwise.
func check(op string, resp *http.Response) ([]byte, error) {
	defer resp.Body.Close()
	data, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}
	if resp.StatusCode/100 == 2 {
		return data, nil
	}
	var e struct {
		Code    string `xml:"Code"`
		Message string `xml:"Message"`
	}
	xml.Unmarshal(data, &e)
	return nil, &s3Error{op: op, status: resp.StatusCode, code: e.Code, msg: e.Message}
}

func status(err error) int {
	if e, ok := err.(*s3Error); ok {
		return e.status
	}
	return 0
}

const amzMeta = "X-Amz-Meta-"

func (b *S3) Get(ctx context.Context, key string) ([]byte, map[string]string, error) {
	resp, err := b.do(ctx, "GET", b.prefix+key, nil, nil, nil)
	if err != nil {
		return nil, nil, err
	}
	data, err := check("GetObject", resp)
	if status(err) == 404 {
		return nil, nil, ErrNotFound
	}
	if err != nil {
		return nil, nil, err
	}
	meta := map[string]string{}
	for k, vs := range resp.Header {
		if strings.HasPrefix(k, amzMeta) && len(vs) > 0 {
			meta[strings.ToLower(k[len(amzMeta):])] = vs[0]
		}
	}
	return data, meta, nil
}

func (b *S3) Stat(ctx context.Context, key string) (ObjectInfo, error) {
	resp, err := b.do(ctx, "HEAD", b.prefix+key, nil, nil, nil)
	if err != nil {
		return ObjectInfo{}, err
	}
	if _, err := check("HeadObject", resp); err != nil {
		if status(err) == 404 {
			return ObjectInfo{}, ErrNotFound
		}
		return ObjectInfo{}, err
	}
	mod, err := http.ParseTime(resp.Header.Get("Last-Modified"))
	if err != nil {
		return ObjectInfo{}, fmt.Errorf("s3 HeadObject %s: Last-Modified: %w", key, err)
	}
	return ObjectInfo{Key: key, Modified: mod}, nil
}

func (b *S3) Create(ctx context.Context, key string, data []byte, meta map[string]string) error {
	hdr := http.Header{"If-None-Match": {"*"}}
	for k, v := range meta {
		hdr.Set(amzMeta+k, v)
	}
	if data == nil {
		data = []byte{}
	}
	// 409 is a conditional write racing another in flight on the same
	// key: retry, and the retry answers 412 or lands.
	for attempt := 0; ; attempt++ {
		resp, err := b.do(ctx, "PUT", b.prefix+key, nil, hdr, data)
		if err != nil {
			return err
		}
		_, err = check("PutObject", resp)
		switch {
		case err == nil:
			return nil
		case status(err) == 412:
			return ErrExists
		case status(err) == 409 && attempt < 5:
			time.Sleep(time.Duration(10<<attempt) * time.Millisecond)
		default:
			return err
		}
	}
}

func (b *S3) List(ctx context.Context, prefix, startAfter string) ([]ObjectInfo, error) {
	q := url.Values{"list-type": {"2"}, "prefix": {b.prefix + prefix}}
	if startAfter != "" {
		q.Set("start-after", b.prefix+startAfter)
	}
	var out []ObjectInfo
	for {
		resp, err := b.do(ctx, "GET", "", q, nil, nil)
		if err != nil {
			return nil, err
		}
		body, err := check("ListObjectsV2", resp)
		if err != nil {
			return nil, err
		}
		var page struct {
			IsTruncated           bool
			NextContinuationToken string
			Contents              []struct {
				Key          string
				LastModified time.Time
			}
		}
		if err := xml.Unmarshal(body, &page); err != nil {
			return nil, fmt.Errorf("s3 ListObjectsV2: %w", err)
		}
		for _, o := range page.Contents {
			out = append(out, ObjectInfo{Key: strings.TrimPrefix(o.Key, b.prefix), Modified: o.LastModified})
		}
		if !page.IsTruncated || page.NextContinuationToken == "" {
			return out, nil
		}
		q.Set("continuation-token", page.NextContinuationToken)
	}
}

func (b *S3) Delete(ctx context.Context, key string) error {
	resp, err := b.do(ctx, "DELETE", b.prefix+key, nil, nil, nil)
	if err != nil {
		return err
	}
	_, err = check("DeleteObject", resp)
	if status(err) == 404 {
		return nil
	}
	return err
}
