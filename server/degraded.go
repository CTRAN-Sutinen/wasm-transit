package main

import (
	"errors"
	"net/http"
	"strconv"

	"rustwasm/server/store"
)

// retryAfterSecs is what a refused write tells the client to wait
// before it resends (D-119).
const retryAfterSecs = 5

// fail answers err: a bucket that is not answering is 503 with
// Retry-After — the write was not taken, the client holds its document
// and resends (D-119); anything else is code.
func fail(w http.ResponseWriter, err error, code int) {
	if errors.Is(err, store.ErrUnavailable) {
		w.Header().Set("Retry-After", strconv.Itoa(retryAfterSecs))
		http.Error(w, "the store is not answering: nothing was written; resend after Retry-After ("+err.Error()+")", http.StatusServiceUnavailable)
		return
	}
	http.Error(w, err.Error(), code)
}

// labelStale marks a read served from a cached head while the bucket
// does not answer: X-Store-Stale, and X-Store-As-Of — when the bucket
// last confirmed the head (Unix ms). The pages show the age (D-119).
func labelStale(w http.ResponseWriter, heads ...store.Head) {
	for _, h := range heads {
		if h.Stale {
			w.Header().Set("X-Store-Stale", "true")
			w.Header().Set("X-Store-As-Of", strconv.FormatInt(h.AsOfMs, 10))
			return
		}
	}
}
