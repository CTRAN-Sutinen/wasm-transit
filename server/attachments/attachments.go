// Package attachments keeps the files a record names. Attachments live in
// a bucket of their own (-attachments), each object named by its content
// hash and created only if absent. A record
// names a file by hash, never a URL. The store's `gc` never touches this
// bucket. Reading needs a signed-in person whenever sign-in is on. Adding
// needs write on the `attachments` pointer of the policy.
package attachments

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"io"
	"net/http"
	"strings"

	"rustwasm/server/httpx"
	"rustwasm/server/store"
)

const MaxBytes = 25 << 20

var attachmentTypes = map[string]bool{
	"image/jpeg": true, "image/png": true, "image/webp": true, "image/heic": true, "image/heif": true,
	"application/pdf": true,
}

// Attachments serve /attachments. Bucket is nil when none is configured.
type Attachments struct {
	Bucket func() store.Backend
}

func (a *Attachments) bucket() store.Backend { return a.Bucket() }

func attachmentKey(hexHash string) string { return "attachments/" + hexHash }

func (a *Attachments) HandleAttachments(w http.ResponseWriter, r *http.Request) {
	if a.bucket() == nil {
		httpx.JSON(w, http.StatusNotImplemented, map[string]any{"configured": false})
		return
	}
	switch r.Method {
	case http.MethodPost:
		a.addAttachment(w, r)
	case http.MethodGet, http.MethodHead:
		a.readAttachment(w, r)
	default:
		http.Error(w, "GET /attachments/<hash> or POST /attachments", http.StatusMethodNotAllowed)
	}
}

// This path is behind sign-in alone, so it must never add a file. POST
// /attachments has the write check.
func (a *Attachments) HandleAttachmentFile(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet && r.Method != http.MethodHead {
		http.Error(w, "GET /attachments/<hash>; add a file with POST /attachments", http.StatusMethodNotAllowed)
		return
	}
	a.HandleAttachments(w, r)
}

func (a *Attachments) addAttachment(w http.ResponseWriter, r *http.Request) {
	media := strings.ToLower(strings.TrimSpace(strings.SplitN(r.Header.Get("Content-Type"), ";", 2)[0]))
	if !attachmentTypes[media] {
		http.Error(w, "an attachment is a photo (JPEG, PNG, WebP, HEIC) or a PDF; "+media+" is not taken", http.StatusUnsupportedMediaType)
		return
	}
	data, err := io.ReadAll(io.LimitReader(r.Body, MaxBytes+1))
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	if len(data) > MaxBytes {
		http.Error(w, "an attachment is at most 25 MB", http.StatusRequestEntityTooLarge)
		return
	}
	if len(data) == 0 {
		http.Error(w, "an empty file", http.StatusBadRequest)
		return
	}
	sum := sha256.Sum256(data)
	hexHash := hex.EncodeToString(sum[:])
	err = a.bucket().Create(r.Context(), attachmentKey(hexHash), data, map[string]string{"type": media})
	if err != nil && !errors.Is(err, store.ErrExists) {
		httpx.Fail(w, err, http.StatusInternalServerError)
		return
	}
	httpx.JSON(w, http.StatusOK, map[string]any{"hash": "sha256:" + hexHash, "type": media, "bytes": len(data)})
}

func (a *Attachments) readAttachment(w http.ResponseWriter, r *http.Request) {
	name := strings.TrimPrefix(strings.TrimPrefix(r.URL.Path, "/attachments/"), "sha256:")
	if len(name) != 64 || strings.Trim(name, "0123456789abcdef") != "" {
		http.Error(w, "GET /attachments/sha256:<64 hex digits>", http.StatusBadRequest)
		return
	}
	data, meta, err := a.bucket().Get(r.Context(), attachmentKey(name))
	if errors.Is(err, store.ErrNotFound) {
		http.NotFound(w, r)
		return
	}
	if err != nil {
		httpx.Fail(w, err, http.StatusInternalServerError)
		return
	}
	etag := `"sha256:` + name + `"`
	w.Header().Set("ETag", etag)
	// The bytes never change, and only the signed-in may hold them.
	w.Header().Set("Cache-Control", "private, max-age=31536000, immutable")
	if r.Header.Get("If-None-Match") == etag {
		w.WriteHeader(http.StatusNotModified)
		return
	}
	w.Header().Set("Content-Type", meta["type"])
	w.Header().Set("X-Content-Type-Options", "nosniff")
	w.Write(data)
}
