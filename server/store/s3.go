package store

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"net/url"
	"strings"
	"time"

	"github.com/aws/aws-sdk-go-v2/aws"
	"github.com/aws/aws-sdk-go-v2/config"
	"github.com/aws/aws-sdk-go-v2/service/s3"
)

// OpenBackend opens the backend a store URL names:
//
//	mem://                                      in memory (dev, tests)
//	s3://bucket[/prefix][?endpoint=URL&region=R] S3 or any S3-compatible
//	                                            endpoint (path-style when
//	                                            endpoint is given)
//
// S3 credentials come from the SDK's default chain (AWS_ACCESS_KEY_ID /
// AWS_SECRET_ACCESS_KEY, a profile, an instance role). There is no
// directory backend: a store needs the object contract (backend.go).
func OpenBackend(ctx context.Context, raw string) (Backend, error) {
	u, err := url.Parse(raw)
	if err != nil {
		return nil, fmt.Errorf("store URL %q: %w", raw, err)
	}
	switch u.Scheme {
	case "mem":
		return NewMemory(), nil
	case "s3":
		if u.Host == "" {
			return nil, fmt.Errorf("store URL %q names no bucket", raw)
		}
		q := u.Query()
		return NewS3(ctx, u.Host, strings.Trim(u.Path, "/"), q.Get("endpoint"), q.Get("region"))
	default:
		return nil, fmt.Errorf("store URL %q: want mem:// or s3://bucket/prefix (the store lives in object storage, D-113)", raw)
	}
}

// S3 is the backend over S3 and S3-compatible endpoints. The
// conditional create is PutObject with If-None-Match: *.
type S3 struct {
	c      *s3.Client
	bucket string
	prefix string // "" or "<prefix>/"
}

// NewS3 answers the backend over bucket, every key under prefix.
// endpoint, when set, names an S3-compatible server (path-style
// addressing); region defaults to the SDK's, else us-east-1.
func NewS3(ctx context.Context, bucket, prefix, endpoint, region string) (*S3, error) {
	cfg, err := config.LoadDefaultConfig(ctx)
	if err != nil {
		return nil, fmt.Errorf("s3 config: %w", err)
	}
	if region != "" {
		cfg.Region = region
	} else if cfg.Region == "" {
		cfg.Region = "us-east-1"
	}
	c := s3.NewFromConfig(cfg, func(o *s3.Options) {
		if endpoint != "" {
			o.BaseEndpoint = aws.String(endpoint)
			o.UsePathStyle = true
		}
		// Checksums only where the API requires them: the contract is the
		// whole-object put, and S3-compatible servers differ on the
		// streaming trailer checksums the SDK otherwise adds.
		o.RequestChecksumCalculation = aws.RequestChecksumCalculationWhenRequired
		o.ResponseChecksumValidation = aws.ResponseChecksumValidationWhenRequired
	})
	if prefix != "" {
		prefix += "/"
	}
	return &S3{c: c, bucket: bucket, prefix: prefix}, nil
}

// status answers an API error's HTTP status (0 when it has none).
func status(err error) int {
	var re interface{ HTTPStatusCode() int }
	if errors.As(err, &re) {
		return re.HTTPStatusCode()
	}
	return 0
}

func (b *S3) Get(ctx context.Context, key string) ([]byte, map[string]string, error) {
	out, err := b.c.GetObject(ctx, &s3.GetObjectInput{Bucket: &b.bucket, Key: aws.String(b.prefix + key)})
	if err != nil {
		if status(err) == 404 {
			return nil, nil, ErrNotFound
		}
		return nil, nil, err
	}
	defer out.Body.Close()
	data, err := io.ReadAll(out.Body)
	if err != nil {
		return nil, nil, err
	}
	meta := map[string]string{}
	for k, v := range out.Metadata {
		meta[strings.ToLower(k)] = v
	}
	return data, meta, nil
}

func (b *S3) Stat(ctx context.Context, key string) (ObjectInfo, error) {
	out, err := b.c.HeadObject(ctx, &s3.HeadObjectInput{Bucket: &b.bucket, Key: aws.String(b.prefix + key)})
	if err != nil {
		if status(err) == 404 {
			return ObjectInfo{}, ErrNotFound
		}
		return ObjectInfo{}, err
	}
	return ObjectInfo{Key: key, Modified: aws.ToTime(out.LastModified)}, nil
}

func (b *S3) Create(ctx context.Context, key string, data []byte, meta map[string]string) error {
	in := &s3.PutObjectInput{
		Bucket:        &b.bucket,
		Key:           aws.String(b.prefix + key),
		Body:          bytes.NewReader(data),
		ContentLength: aws.Int64(int64(len(data))),
		IfNoneMatch:   aws.String("*"),
		Metadata:      meta,
	}
	// 409 is a conditional write racing another in flight on the same
	// key: retry, and the retry answers 412 or lands.
	for attempt := 0; ; attempt++ {
		_, err := b.c.PutObject(ctx, in)
		switch {
		case err == nil:
			return nil
		case status(err) == 412:
			return ErrExists
		case status(err) == 409 && attempt < 5:
			in.Body = bytes.NewReader(data)
			time.Sleep(time.Duration(10<<attempt) * time.Millisecond)
		default:
			return err
		}
	}
}

func (b *S3) List(ctx context.Context, prefix, startAfter string) ([]ObjectInfo, error) {
	in := &s3.ListObjectsV2Input{Bucket: &b.bucket, Prefix: aws.String(b.prefix + prefix)}
	if startAfter != "" {
		in.StartAfter = aws.String(b.prefix + startAfter)
	}
	var out []ObjectInfo
	pages := s3.NewListObjectsV2Paginator(b.c, in)
	for pages.HasMorePages() {
		page, err := pages.NextPage(ctx)
		if err != nil {
			return nil, err
		}
		for _, o := range page.Contents {
			out = append(out, ObjectInfo{
				Key:      strings.TrimPrefix(aws.ToString(o.Key), b.prefix),
				Modified: aws.ToTime(o.LastModified),
			})
		}
	}
	return out, nil
}

func (b *S3) Delete(ctx context.Context, key string) error {
	_, err := b.c.DeleteObject(ctx, &s3.DeleteObjectInput{Bucket: &b.bucket, Key: aws.String(b.prefix + key)})
	if status(err) == 404 {
		return nil
	}
	return err
}
