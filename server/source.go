package main

import (
	"errors"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"strings"
	"sync"
	"time"

	"rustwasm/server/store"
)

// source is where a server reads one input: an http(s) URL,
// bucket:<key> (the object inputs/<key>), or a local path for dev. Every
// read that lands is committed as the head of its pointer (source.feed,
// source.streets). A source that does not answer serves the last read,
// else the store's head, so a replacement process serves the same inputs.
type source struct {
	raw     string
	pointer string // "" = not committed (the policy: it seeds its own pointer)
	kind    string // the committed blob's envelope kind
	st      *store.Store
	every   time.Duration // a URL or bucket source re-checks at most this often
	client  *http.Client

	mu      sync.Mutex
	checked time.Time
	tag     string // what "unchanged" is: an ETag, a modified time
	body    []byte
	hash    string
}

func newSource(raw, pointer, kind string, st *store.Store) *source {
	return &source{raw: raw, pointer: pointer, kind: kind, st: st,
		every: time.Minute, client: &http.Client{Transport: outbound, Timeout: 2 * time.Minute}}
}

func (s *source) Read() ([]byte, string, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	remote := s.isRemote()
	if s.body != nil && remote && time.Since(s.checked) < s.every {
		return s.body, s.hash, nil
	}
	body, tag, err := s.fetch()
	s.checked = time.Now()
	switch {
	case err == nil && body == nil: // unchanged
		return s.body, s.hash, nil
	case err == nil:
		s.body, s.tag, s.hash = body, tag, store.HashOf(body)
		s.commit()
		return s.body, s.hash, nil
	case s.body != nil:
		log.Printf("source %s did not answer (%v): serving the last read", s.raw, err)
		return s.body, s.hash, nil
	}
	if s.st != nil && s.pointer != "" {
		if head, found, herr := s.st.Head(s.pointer); herr == nil && found {
			if data, _, gerr := s.st.GetBlob(head.Hash); gerr == nil {
				log.Printf("source %s did not answer (%v): serving the store's last import (%s seq %d)", s.raw, err, s.pointer, head.Seq)
				s.body, s.hash = data, head.Hash
				return s.body, s.hash, nil
			}
		}
	}
	return nil, "", fmt.Errorf("source %s: %w (and the store holds no earlier import)", s.raw, err)
}

func (s *source) isRemote() bool {
	return strings.HasPrefix(s.raw, "http://") || strings.HasPrefix(s.raw, "https://") || strings.HasPrefix(s.raw, "bucket:")
}

// fetch answers nil bytes and no error when the source is unchanged since tag.
func (s *source) fetch() ([]byte, string, error) {
	switch {
	case strings.HasPrefix(s.raw, "http://"), strings.HasPrefix(s.raw, "https://"):
		req, err := http.NewRequest(http.MethodGet, s.raw, nil)
		if err != nil {
			return nil, "", err
		}
		if s.body != nil && s.tag != "" {
			req.Header.Set("If-None-Match", s.tag)
		}
		resp, err := s.client.Do(req)
		if err != nil {
			return nil, "", err
		}
		defer resp.Body.Close()
		if resp.StatusCode == http.StatusNotModified && s.body != nil {
			return nil, "", nil
		}
		if resp.StatusCode != http.StatusOK {
			return nil, "", fmt.Errorf("answered %s", resp.Status)
		}
		body, err := io.ReadAll(resp.Body)
		return body, resp.Header.Get("ETag"), err
	case strings.HasPrefix(s.raw, "bucket:"):
		if s.st == nil {
			return nil, "", errors.New("bucket: needs -store")
		}
		key := strings.TrimPrefix(s.raw, "bucket:")
		if s.body != nil {
			if info, err := s.st.InputModified(key); err == nil && info.Modified.String() == s.tag {
				return nil, "", nil
			}
		}
		body, info, err := s.st.Input(key)
		return body, info.Modified.String(), err
	default:
		info, err := os.Stat(s.raw)
		if err != nil {
			return nil, "", err
		}
		tag := info.ModTime().String()
		if s.body != nil && tag == s.tag {
			return nil, "", nil
		}
		body, err := os.ReadFile(s.raw)
		return body, tag, err
	}
}

// A failed commit is logged, never fatal: the read stands, and the next one
// tries again.
func (s *source) commit() {
	if s.st == nil || s.pointer == "" {
		return
	}
	head, found, err := s.st.Head(s.pointer)
	if err == nil && found && head.Hash == s.hash {
		return
	}
	from := ""
	if found {
		from = head.Hash
	}
	env := store.Envelope{Kind: s.kind, SchemaVersion: 1, ProducedBy: "server"}
	if _, _, err := s.st.CAS(s.pointer, from, s.body, env, "", "import:"+s.raw, time.Now().UnixMilli()); err != nil {
		var ce *store.ConflictError
		if !errors.As(err, &ce) { // another process committed first: its read stands
			log.Printf("source %s: commit to %s: %v", s.raw, s.pointer, err)
		}
	}
}
