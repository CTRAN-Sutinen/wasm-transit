package main

import (
	"encoding/json"
	"errors"
	"net/http"
	"strconv"

	"rustwasm/server/store"
	"rustwasm/server/wasmhost"
)

const retryAfterSecs = 5

// fail answers 503 with Retry-After for an unavailable bucket, because nothing
// was written and the client resends (D-119). Other errors use code.
func fail(w http.ResponseWriter, err error, code int) {
	if errors.Is(err, store.ErrUnavailable) {
		w.Header().Set("Retry-After", strconv.Itoa(retryAfterSecs))
		http.Error(w, "the store is not answering: nothing was written; resend after Retry-After ("+err.Error()+")", http.StatusServiceUnavailable)
		return
	}
	if refusal, ok := wasmhost.CodedRefusal(err); ok {
		answerFailed(w, refusal, code, "")
		return
	}
	http.Error(w, err.Error(), code)
}

// answerFailed answers a module's error. One with a code is JSON, {"error":
// {...}}, so a page acts on the code (docs/architecture/bus.md, "Answers").
// Other errors are text for a person.
func answerFailed(w http.ResponseWriter, e wasmhost.ModuleError, code int, prefix string) {
	if e.Code != "" {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(code)
		json.NewEncoder(w).Encode(map[string]wasmhost.ModuleError{"error": e})
		return
	}
	http.Error(w, prefix+e.Message, code)
}

// X-Store-As-Of is when the bucket last confirmed the head, in Unix ms (D-119).
func labelStale(w http.ResponseWriter, heads ...store.Head) {
	for _, h := range heads {
		if h.Stale {
			w.Header().Set("X-Store-Stale", "true")
			w.Header().Set("X-Store-As-Of", strconv.FormatInt(h.AsOfMs, 10))
			return
		}
	}
}
