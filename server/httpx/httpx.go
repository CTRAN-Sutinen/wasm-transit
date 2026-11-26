// Package httpx holds the answers that every route shares: a failure, a
// module's error, a stale head, fields this build does not know, and who
// wrote the request.
package httpx

import (
	"context"
	"encoding/json"
	"errors"
	"log"
	"net/http"
	"strconv"
	"strings"

	"rustwasm/server/store"
	"rustwasm/server/wasmhost"
)

const RetryAfterSecs = 5

// Outbound is http.DefaultTransport as it stands when the server starts.
// Tests replace http.DefaultTransport to check their own requests against
// the HTTP contract, and the server's outgoing requests must not pass
// through that check.
var Outbound = http.DefaultTransport

// Fail answers 503 with Retry-After for an unavailable bucket, because nothing
// was written and the client resends. Other errors use code.
func Fail(w http.ResponseWriter, err error, code int) {
	if errors.Is(err, store.ErrUnavailable) {
		w.Header().Set("Retry-After", strconv.Itoa(RetryAfterSecs))
		http.Error(w, "the store is not answering: nothing was written; resend after Retry-After ("+err.Error()+")", http.StatusServiceUnavailable)
		return
	}
	if refusal, ok := wasmhost.CodedRefusal(err); ok {
		AnswerFailed(w, refusal, code, "")
		return
	}
	http.Error(w, err.Error(), code)
}

// AnswerFailed answers a module's error. One with a code is JSON, {"error":
// {...}}, so a page acts on the code (docs/architecture/bus.md, "Answers").
// Other errors are text for a person.
func AnswerFailed(w http.ResponseWriter, e wasmhost.ModuleError, code int, prefix string) {
	if e.Code != "" {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(code)
		json.NewEncoder(w).Encode(map[string]wasmhost.ModuleError{"error": e})
		return
	}
	http.Error(w, prefix+e.Message, code)
}

// LabelStale sets X-Store-As-Of, when the bucket last confirmed the head,
// in Unix ms.
func LabelStale(w http.ResponseWriter, heads ...store.Head) {
	for _, h := range heads {
		if h.Stale {
			w.Header().Set("X-Store-Stale", "true")
			w.Header().Set("X-Store-As-Of", strconv.FormatInt(h.AsOfMs, 10))
			return
		}
	}
}

func JSON(w http.ResponseWriter, code int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(code)
	json.NewEncoder(w).Encode(v)
}

// A write may carry fields that this build does not know. Every
// authored kind keeps them, so a writer on an older build does not drop
// what a newer one wrote. The write path never refuses one. It flags
// the field for a person.
const UnknownFieldsHeader = "X-Unknown-Fields"

func FlagUnknown(w http.ResponseWriter, what, writer string, names []string) {
	if len(names) == 0 {
		return
	}
	w.Header().Set(UnknownFieldsHeader, strings.Join(names, ", "))
	log.Printf("receive %s: %s wrote fields this build does not know, kept: %s", what, writer, strings.Join(names, ", "))
}

type ctxKey string

const (
	writerKey ctxKey = "writer"
	nameKey   ctxKey = "name"
)

// WithWriter names the verified writer (the token's sub).
func WithWriter(ctx context.Context, writer string) context.Context {
	return context.WithValue(ctx, writerKey, writer)
}

// WithName names the verified person (the token's name claim).
func WithName(ctx context.Context, name string) context.Context {
	return context.WithValue(ctx, nameKey, name)
}

func Writer(r *http.Request, fallback string) string {
	if v, ok := r.Context().Value(writerKey).(string); ok && v != "" {
		return v
	}
	if h := r.Header.Get("X-Writer"); h != "" {
		return h
	}
	return fallback
}

// Name prefers the verified token's name claim.
func Name(r *http.Request, fallback string) string {
	if v, ok := r.Context().Value(nameKey).(string); ok && v != "" {
		return v
	}
	return Writer(r, fallback)
}
