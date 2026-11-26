package store

import (
	"errors"
	"time"
)

// Another process that committed the same bytes first is a success. A
// derivation of the same inputs makes the same bytes, so another head is
// retried over a few times.
func (st *Store) CommitHead(pointer string, body []byte, env Envelope, wantHash, writer string) error {
	for attempt := 0; ; attempt++ {
		head, found, err := st.Head(pointer)
		if err != nil {
			return err
		}
		from := ""
		if found {
			from = head.Hash
		}
		_, _, err = st.CAS(pointer, from, body, env, wantHash, writer, time.Now().UnixMilli())
		var ce *ConflictError
		switch {
		case err == nil:
			return nil
		case !errors.As(err, &ce):
			return err
		case ce.Current == HashOf(body):
			return nil
		case attempt >= 2:
			return err
		}
	}
}
