package store

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/Azure/azure-sdk-for-go/sdk/azcore"
	"github.com/Azure/azure-sdk-for-go/sdk/azidentity"
	"github.com/Azure/azure-sdk-for-go/sdk/storage/azblob"
	"github.com/Azure/azure-sdk-for-go/sdk/storage/azblob/blob"
	"github.com/Azure/azure-sdk-for-go/sdk/storage/azblob/bloberror"
	"github.com/Azure/azure-sdk-for-go/sdk/storage/azblob/container"
)

// Azure is the backend over Azure Blob Storage. The conditional create
// is Put Blob with If-None-Match: *.
type Azure struct {
	c         *azblob.Client
	container string
	prefix    string // "" or "<prefix>/"
}

// NewAzure answers the backend over a container, every key under
// prefix. Credentials, first found: AZURE_STORAGE_CONNECTION_STRING;
// AZURE_STORAGE_ACCOUNT with AZURE_STORAGE_KEY; else the account with
// the default Azure credential chain (managed identity, workload
// identity, the CLI). endpoint, when set, is the blob service URL.
func NewAzure(containerName, prefix, endpoint string) (*Azure, error) {
	var c *azblob.Client
	var err error
	account := os.Getenv("AZURE_STORAGE_ACCOUNT")
	if endpoint == "" && account != "" {
		endpoint = "https://" + account + ".blob.core.windows.net/"
	}
	switch {
	case os.Getenv("AZURE_STORAGE_CONNECTION_STRING") != "":
		c, err = azblob.NewClientFromConnectionString(os.Getenv("AZURE_STORAGE_CONNECTION_STRING"), nil)
	case endpoint == "":
		return nil, fmt.Errorf("azblob: no endpoint: set AZURE_STORAGE_ACCOUNT, ?endpoint=, or AZURE_STORAGE_CONNECTION_STRING")
	case os.Getenv("AZURE_STORAGE_KEY") != "":
		var cred *azblob.SharedKeyCredential
		if cred, err = azblob.NewSharedKeyCredential(account, os.Getenv("AZURE_STORAGE_KEY")); err == nil {
			c, err = azblob.NewClientWithSharedKeyCredential(endpoint, cred, nil)
		}
	default:
		var cred *azidentity.DefaultAzureCredential
		if cred, err = azidentity.NewDefaultAzureCredential(nil); err == nil {
			c, err = azblob.NewClient(endpoint, cred, nil)
		}
	}
	if err != nil {
		return nil, fmt.Errorf("azblob: %w", err)
	}
	if prefix != "" {
		prefix += "/"
	}
	return &Azure{c: c, container: containerName, prefix: prefix}, nil
}

func (a *Azure) blob(key string) *blob.Client {
	return a.c.ServiceClient().NewContainerClient(a.container).NewBlobClient(a.prefix + key)
}

func lowerMeta(m map[string]*string) map[string]string {
	out := map[string]string{}
	for k, v := range m {
		if v != nil {
			out[strings.ToLower(k)] = *v
		}
	}
	return out
}

func (a *Azure) Get(ctx context.Context, key string) ([]byte, map[string]string, error) {
	out, err := a.c.DownloadStream(ctx, a.container, a.prefix+key, nil)
	if err != nil {
		if bloberror.HasCode(err, bloberror.BlobNotFound) {
			return nil, nil, ErrNotFound
		}
		return nil, nil, err
	}
	defer out.Body.Close()
	data, err := io.ReadAll(out.Body)
	if err != nil {
		return nil, nil, err
	}
	return data, lowerMeta(out.Metadata), nil
}

func (a *Azure) Stat(ctx context.Context, key string) (ObjectInfo, error) {
	out, err := a.blob(key).GetProperties(ctx, nil)
	if err != nil {
		if bloberror.HasCode(err, bloberror.BlobNotFound) || status(err) == 404 {
			return ObjectInfo{}, ErrNotFound
		}
		return ObjectInfo{}, err
	}
	info := ObjectInfo{Key: key}
	if out.LastModified != nil {
		info.Modified = *out.LastModified
	}
	return info, nil
}

func (a *Azure) Create(ctx context.Context, key string, data []byte, meta map[string]string) error {
	md := map[string]*string{}
	for k, v := range meta {
		md[k] = &v
	}
	anyTag := azcore.ETagAny
	_, err := a.c.UploadStream(ctx, a.container, a.prefix+key, bytes.NewReader(data), &azblob.UploadStreamOptions{
		Metadata: md,
		AccessConditions: &blob.AccessConditions{
			ModifiedAccessConditions: &blob.ModifiedAccessConditions{IfNoneMatch: &anyTag},
		},
	})
	if bloberror.HasCode(err, bloberror.BlobAlreadyExists, bloberror.ConditionNotMet) || status(err) == 412 {
		return ErrExists
	}
	return err
}

func (a *Azure) List(ctx context.Context, prefix, startAfter string) ([]ObjectInfo, error) {
	opts := &container.ListBlobsFlatOptions{Prefix: strPtr(a.prefix + prefix)}
	if startAfter != "" {
		opts.StartFrom = strPtr(a.prefix + startAfter) // inclusive, and a hint: filtered below
	}
	var out []ObjectInfo
	pages := a.c.NewListBlobsFlatPager(a.container, opts)
	for pages.More() {
		page, err := pages.NextPage(ctx)
		if err != nil {
			return nil, err
		}
		for _, item := range page.Segment.BlobItems {
			key := strings.TrimPrefix(*item.Name, a.prefix)
			if key <= startAfter {
				continue
			}
			info := ObjectInfo{Key: key}
			if item.Properties != nil && item.Properties.LastModified != nil {
				info.Modified = *item.Properties.LastModified
			}
			out = append(out, info)
		}
	}
	return out, nil
}

func (a *Azure) Delete(ctx context.Context, key string) error {
	_, err := a.c.DeleteBlob(ctx, a.container, a.prefix+key, nil)
	if bloberror.HasCode(err, bloberror.BlobNotFound) {
		return nil
	}
	return err
}

func strPtr(s string) *string { return &s }
