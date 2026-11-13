package main

// Attachments (D-162): a bucket of their own, each file named by its
// content hash; reading needs a signed-in person when sign-in is on.

import (
	"bytes"
	"crypto/rand"
	"crypto/rsa"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"rustwasm/server/store"
)

func postAttachment(t *testing.T, url, token, media string, data []byte) (int, map[string]any) {
	t.Helper()
	req, _ := http.NewRequest("POST", url, bytes.NewReader(data))
	req.Header.Set("Content-Type", media)
	if token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	var out map[string]any
	b, _ := io.ReadAll(resp.Body)
	json.Unmarshal(b, &out)
	return resp.StatusCode, out
}

func getAttachment(t *testing.T, url, token string) (int, string, []byte) {
	t.Helper()
	req, _ := http.NewRequest("GET", url, nil)
	if token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	b, _ := io.ReadAll(resp.Body)
	return resp.StatusCode, resp.Header.Get("Content-Type"), b
}

func attachmentMux(s *server) *http.ServeMux {
	mux := http.NewServeMux()
	mux.HandleFunc("/attachments", s.protectWrite(func(*http.Request) string { return "attachments" }, s.handleAttachments))
	mux.HandleFunc("/attachments/", s.protectSignedIn(s.handleAttachments))
	return mux
}

func TestAttachmentsAreKeptByHash(t *testing.T) {
	s := &server{}
	srv := httptest.NewServer(attachmentMux(s))
	defer srv.Close()
	photo := []byte("\xff\xd8\xff a photo of the stop")
	if code, out := postAttachment(t, srv.URL+"/attachments", "", "image/jpeg", photo); code != 501 || out["configured"] != false {
		t.Fatalf("no bucket: %d %v", code, out)
	}
	s.attachments = store.NewMemory()
	code, out := postAttachment(t, srv.URL+"/attachments", "", "image/jpeg", photo)
	if code != 200 || out["hash"] != sha(photo) || out["type"] != "image/jpeg" {
		t.Fatalf("a photo: %d %v", code, out)
	}
	if code, again := postAttachment(t, srv.URL+"/attachments", "", "image/jpeg", photo); code != 200 || again["hash"] != out["hash"] {
		t.Fatalf("the same photo twice is one: %d %v", code, again)
	}
	gotCode, media, body := getAttachment(t, srv.URL+"/attachments/"+out["hash"].(string), "")
	if gotCode != 200 || media != "image/jpeg" || !bytes.Equal(body, photo) {
		t.Fatalf("read back: %d %q %q", gotCode, media, body)
	}
	if code, _, _ := getAttachment(t, srv.URL+"/attachments/sha256:"+strings.Repeat("0", 64), ""); code != 404 {
		t.Fatalf("an unknown hash: %d", code)
	}
	if code, _ := postAttachment(t, srv.URL+"/attachments", "", "text/html", []byte("<script>")); code != 415 {
		t.Fatalf("a type not taken: %d", code)
	}
	if code, _ := postAttachment(t, srv.URL+"/attachments", "", "application/pdf", make([]byte, maxAttachment+1)); code != 413 {
		t.Fatalf("too large: %d", code)
	}
}

func TestAttachmentsAreReadOnlySignedIn(t *testing.T) {
	rsaKey, _ := rsa.GenerateKey(rand.Reader, 2048)
	jwks := newJWKSServer([]map[string]any{rsaJWK("k1", &rsaKey.PublicKey)})
	defer jwks.srv.Close()
	s, _ := newAuthServer(t, jwks)
	s.attachments = store.NewMemory()
	srv := httptest.NewServer(attachmentMux(s))
	defer srv.Close()
	token := func(roles ...string) string {
		return signRS256(t, rsaKey, "k1", map[string]any{"iss": "https://idp.example", "aud": "transit-server", "sub": "u1", "roles": roles, "exp": time.Now().Unix() + 3600})
	}
	photo := []byte("\x89PNG a drawing")
	if code, _ := postAttachment(t, srv.URL+"/attachments", "", "image/png", photo); code != 401 {
		t.Fatalf("adding one needs sign-in: %d", code)
	}
	if code, _ := postAttachment(t, srv.URL+"/attachments", token("dispatcher"), "image/png", photo); code != 403 {
		t.Fatalf("adding one needs write on attachments: %d", code)
	}
	code, out := postAttachment(t, srv.URL+"/attachments", token("admin"), "image/png", photo)
	if code != 200 {
		t.Fatalf("added: %d %v", code, out)
	}
	url := srv.URL + "/attachments/" + out["hash"].(string)
	if code, _, _ := getAttachment(t, url, ""); code != 401 {
		t.Fatalf("reading needs sign-in: %d", code)
	}
	if code, _, body := getAttachment(t, url, token("dispatcher")); code != 200 || !bytes.Equal(body, photo) {
		t.Fatalf("any signed-in person reads it: %d", code)
	}
}
