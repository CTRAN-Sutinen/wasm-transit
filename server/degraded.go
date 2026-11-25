package main

import (
	"errors"
	"net/http"
	"strconv"

	"rustwasm/server/store"
)

const retryAfterSecs = 5

// fail answers 503 with Retry-After for an unavailable bucket, because
// nothing was written and the client resends (D-119). Other errors use
// code.
func fail(w http.ResponseWriter, err error, code int) {
	if errors.Is(err, store.ErrUnavailable) {
		w.Header().Set("Retry-After", strconv.Itoa(retryAfterSecs))
		http.Error(w, "the store is not answering: nothing was written; resend after Retry-After ("+err.Error()+")", http.StatusServiceUnavailable)
		return
	}
	http.Error(w, err.Error(), code)
}

// X-Store-As-Of is when the bucket last confirmed the head, in Unix ms
// (D-119).
func labelStale(w http.ResponseWriter, heads ...store.Head) {
	for _, h := range heads {
		if h.Stale {
			w.Header().Set("X-Store-Stale", "true")
			w.Header().Set("X-Store-As-Of", strconv.FormatInt(h.AsOfMs, 10))
			return
		}
	}
}
