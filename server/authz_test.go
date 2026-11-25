package main

// Authorization round trips with a test JWKS (RS256 and ES256): the
// policy is fail-closed, reads are public, writes are role-gated
// (D-58..D-61), and the verified subject lands in the log as `writer`.

import (
	"context"
	"crypto"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/rsa"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"io"
	"math/big"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"rustwasm/server/store"
)

func b64enc(b []byte) string { return base64.RawURLEncoding.EncodeToString(b) }

func signRS256(t *testing.T, key *rsa.PrivateKey, kid string, claims map[string]any) string {
	t.Helper()
	h, _ := json.Marshal(map[string]any{"alg": "RS256", "typ": "JWT", "kid": kid})
	c, _ := json.Marshal(claims)
	input := b64enc(h) + "." + b64enc(c)
	digest := sha256.Sum256([]byte(input))
	sig, err := rsa.SignPKCS1v15(rand.Reader, key, crypto.SHA256, digest[:])
	if err != nil {
		t.Fatal(err)
	}
	return input + "." + b64enc(sig)
}

func signES256(t *testing.T, key *ecdsa.PrivateKey, kid string, claims map[string]any) string {
	t.Helper()
	h, _ := json.Marshal(map[string]any{"alg": "ES256", "typ": "JWT", "kid": kid})
	c, _ := json.Marshal(claims)
	input := b64enc(h) + "." + b64enc(c)
	digest := sha256.Sum256([]byte(input))
	r, s, err := ecdsa.Sign(rand.Reader, key, digest[:])
	if err != nil {
		t.Fatal(err)
	}
	sig := make([]byte, 64)
	r.FillBytes(sig[:32])
	s.FillBytes(sig[32:])
	return input + "." + b64enc(sig)
}

func rsaJWK(kid string, key *rsa.PublicKey) map[string]any {
	e := big.NewInt(int64(key.E)).Bytes()
	return map[string]any{
		"kid": kid, "kty": "RSA", "alg": "RS256", "use": "sig",
		"n": b64enc(key.N.Bytes()), "e": b64enc(e),
	}
}

func ecJWK(kid string, key *ecdsa.PublicKey) map[string]any {
	x := make([]byte, 32)
	y := make([]byte, 32)
	key.X.FillBytes(x)
	key.Y.FillBytes(y)
	return map[string]any{
		"kid": kid, "kty": "EC", "alg": "ES256", "use": "sig",
		"crv": "P-256", "x": b64enc(x), "y": b64enc(y),
	}
}

type jwksServer struct {
	mu   sync.Mutex
	keys []map[string]any
	srv  *httptest.Server
}

func newJWKSServer(initial []map[string]any) *jwksServer {
	j := &jwksServer{keys: initial}
	j.srv = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		j.mu.Lock()
		defer j.mu.Unlock()
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{"keys": j.keys})
	}))
	return j
}

func (j *jwksServer) add(k map[string]any) {
	j.mu.Lock()
	defer j.mu.Unlock()
	j.keys = append(j.keys, k)
}

func (j *jwksServer) url() string { return j.srv.URL }

func testPolicy() []byte {
	return []byte(`{"schema_version":1,"rules":[
		{"name":"ops","roles":["dispatcher"],"pointers":["edits"],"actions":["read","write"]},
		{"name":"auditors","roles":["auditor"],"pointers":["*"],"actions":["audit"]},
		{"name":"admin-all","roles":["admin"],"pointers":["*"],"actions":["*"]}
	]}`)
}

func newAuthzTestHost(t *testing.T) *WasmHost {
	t.Helper()
	return testView(t, "authz")
}

func newAuthServer(t *testing.T, jwks *jwksServer) (*server, string) {
	t.Helper()
	dir := t.TempDir()
	s := &server{wasm: newTestHost(t), retentionDays: 90}
	st := store.NewServing(store.NewMemory())
	s.store = st
	s.seedStore("")
	policyPath := filepath.Join(dir, "policy.json")
	if err := os.WriteFile(policyPath, testPolicy(), 0o644); err != nil {
		t.Fatal(err)
	}
	s.seedPolicy(policyPath)

	a := &authzState{
		cfg: oidcConfig{
			issuer: "https://idp.example", jwksURL: jwks.url(),
			audience: "transit-server", rolesClaim: "roles",
		},
		wasm:            newAuthzTestHost(t),
		jwksKeys:        map[string]crypto.PublicKey{},
		jwksMinInterval: 0, // tests rotate keys immediately
	}
	s.authz = a
	t.Cleanup(func() { st.Close() })

	mux := http.NewServeMux()
	mux.HandleFunc("/edits", s.protectWrite(func(*http.Request) string { return "edits" }, s.handleEdits))
	mux.HandleFunc("/pointers/", s.protectWrite(pointerNameOf, s.handlePointers))
	mux.HandleFunc("/logs/", s.protectLog(s.handleLogs))
	mux.HandleFunc("/blobs/", s.handleBlob)
	base := httptest.NewServer(mux).URL
	return s, base
}

func bearerPost(t *testing.T, url, token, body string) (int, string) {
	t.Helper()
	req, _ := http.NewRequest("POST", url, strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	if token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	b, _ := io.ReadAll(resp.Body)
	return resp.StatusCode, string(b)
}

func TestAuthzPublicReadAuthedWrite(t *testing.T) {
	rsaKey, _ := rsa.GenerateKey(rand.Reader, 2048)
	jwks := newJWKSServer([]map[string]any{rsaJWK("k1", &rsaKey.PublicKey)})
	defer jwks.srv.Close()
	_, base := newAuthServer(t, jwks)

	now := time.Now().Unix()
	claims := func(roles []string, extra map[string]any) map[string]any {
		c := map[string]any{
			"iss": "https://idp.example", "aud": "transit-server",
			"sub": "dispatcher-7", "roles": roles,
			"exp": now + 3600, "nbf": now - 10,
		}
		for k, v := range extra {
			c[k] = v
		}
		return c
	}

	code, body := func() (int, string) {
		resp, err := http.Get(base + "/edits")
		if err != nil {
			t.Fatal(err)
		}
		defer resp.Body.Close()
		b, _ := io.ReadAll(resp.Body)
		return resp.StatusCode, string(b)
	}()
	if code != 200 {
		t.Fatalf("GET /edits must be public: %d %s", code, body)
	}

	if code, _ := bearerPost(t, base+"/edits", "", `[{"id":"e1"}]`); code != 401 {
		t.Fatalf("no token: want 401, got %d", code)
	}
	if code, _ := bearerPost(t, base+"/edits", "eyJhbGciOiJub25lIn0.e30.", `[]`); code != 401 {
		t.Fatalf("alg=none must be rejected: %d", code)
	}
	expired := signRS256(t, rsaKey, "k1", claims([]string{"dispatcher"}, map[string]any{"exp": now - 3600}))
	if code, _ := bearerPost(t, base+"/edits", expired, `[]`); code != 401 {
		t.Fatalf("expired token: want 401, got %d", code)
	}
	wrongAud := signRS256(t, rsaKey, "k1", claims([]string{"dispatcher"}, map[string]any{"aud": "other"}))
	if code, _ := bearerPost(t, base+"/edits", wrongAud, `[]`); code != 401 {
		t.Fatalf("wrong aud: want 401, got %d", code)
	}
	rider := signRS256(t, rsaKey, "k1", claims([]string{"rider"}, nil))
	if code, _ := bearerPost(t, base+"/edits", rider, editDoc("e1", "draft", 1, 100)); code != 403 {
		t.Fatalf("rider writing edits: want 403, got %d", code)
	}
	dispatcher := signRS256(t, rsaKey, "k1", claims([]string{"dispatcher"}, nil))
	code, body = bearerPost(t, base+"/edits", dispatcher, editDoc("e1", "draft", 1, 100))
	if code != 200 || !strings.Contains(body, `"submitted"`) {
		t.Fatalf("dispatcher post: %d %s", code, body)
	}
}

func TestAuthzWriterIsTheVerifiedSubject(t *testing.T) {
	rsaKey, _ := rsa.GenerateKey(rand.Reader, 2048)
	jwks := newJWKSServer([]map[string]any{rsaJWK("k1", &rsaKey.PublicKey)})
	defer jwks.srv.Close()
	s, base := newAuthServer(t, jwks)

	now := time.Now().Unix()
	tok := signRS256(t, rsaKey, "k1", map[string]any{
		"iss": "https://idp.example", "aud": "transit-server",
		"sub": "dispatcher-7", "roles": []string{"dispatcher"},
		"exp": now + 3600,
	})
	if code, _ := bearerPost(t, base+"/edits", tok, editDoc("e1", "draft", 1, 100)); code != 200 {
		t.Fatalf("post: %d", code)
	}
	entries, err := s.store.Entries(disruptionsLog, 0, 0)
	if err != nil || len(entries) == 0 {
		t.Fatalf("log: %d %v", len(entries), err)
	}
	var last struct {
		Writer string `json:"writer"`
	}
	json.Unmarshal([]byte(entries[len(entries)-1].Text), &last)
	if last.Writer != "dispatcher-7" {
		t.Fatalf("log writer = %q, want the verified sub", last.Writer)
	}
}

func TestAuthzES256AndKidRotation(t *testing.T) {
	ecKey, _ := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	jwks := newJWKSServer([]map[string]any{ecJWK("ec1", &ecKey.PublicKey)})
	defer jwks.srv.Close()
	_, base := newAuthServer(t, jwks)

	now := time.Now().Unix()
	claims := map[string]any{
		"iss": "https://idp.example", "aud": "transit-server",
		"sub": "dispatcher-9", "roles": []string{"dispatcher"},
		"exp": now + 3600,
	}
	tok := signES256(t, ecKey, "ec1", claims)
	if code, _ := bearerPost(t, base+"/edits", tok, editDoc("e9", "draft", 1, 100)); code != 200 {
		t.Fatalf("ES256 post: want 200, got %d", code)
	}
	rsaKey, _ := rsa.GenerateKey(rand.Reader, 2048)
	rotated := signRS256(t, rsaKey, "k2", claims)
	if code, _ := bearerPost(t, base+"/edits", rotated, `[]`); code != 401 {
		t.Fatalf("unknown kid: want 401, got %d", code)
	}
	jwks.add(rsaJWK("k2", &rsaKey.PublicKey))
	if code, body := bearerPost(t, base+"/edits", rotated, editDoc("e10", "draft", 1, 100)); code != 200 {
		t.Fatalf("after kid rotation: want 200, got %d %s", code, body)
	}
}

func TestAuthzPolicyChangesAreAuditedCommits(t *testing.T) {
	rsaKey, _ := rsa.GenerateKey(rand.Reader, 2048)
	jwks := newJWKSServer([]map[string]any{rsaJWK("k1", &rsaKey.PublicKey)})
	defer jwks.srv.Close()
	s, base := newAuthServer(t, jwks)

	now := time.Now().Unix()
	admin := signRS256(t, rsaKey, "k1", map[string]any{
		"iss": "https://idp.example", "aud": "transit-server",
		"sub": "admin-1", "roles": []string{"admin"}, "exp": now + 3600,
	})
	dispatcher := signRS256(t, rsaKey, "k1", map[string]any{
		"iss": "https://idp.example", "aud": "transit-server",
		"sub": "dispatcher-7", "roles": []string{"dispatcher"}, "exp": now + 3600,
	})

	newPolicy := `{"schema_version":1,"rules":[{"name":"admin-all","roles":["admin"],"pointers":["*"],"actions":["*"]}]}`
	req, _ := json.Marshal(map[string]any{"from": mustHead(t, s, "policy"), "kind": "policy", "document": newPolicy})
	code, body := bearerPost(t, base+"/pointers/policy", admin, string(req))
	if code != 200 {
		t.Fatalf("policy rewrite: %d %s", code, body)
	}
	if code, _ := bearerPost(t, base+"/edits", dispatcher, editDoc("e1", "draft", 1, 100)); code != 403 {
		t.Fatalf("dispatcher after policy change: want 403, got %d", code)
	}
	entries, _, err := s.store.Log("policy", "")
	if err != nil || len(entries) != 2 || entries[1].Writer != "admin-1" || entries[1].Kind != "policy" {
		t.Fatalf("policy log: %v %v", entries, err)
	}
	if head, found, _ := s.store.Head("policy"); !found || head.Hash != entries[1].To {
		t.Fatalf("policy head: %+v", head)
	}
}

func TestAuthzFailClosedWithoutPolicy(t *testing.T) {
	rsaKey, _ := rsa.GenerateKey(rand.Reader, 2048)
	jwks := newJWKSServer([]map[string]any{rsaJWK("k1", &rsaKey.PublicKey)})
	defer jwks.srv.Close()

	s := &server{wasm: newTestHost(t), retentionDays: 90}
	st := store.NewServing(store.NewMemory())
	s.store = st
	s.seedStore("")
	s.authz = &authzState{
		cfg:             oidcConfig{issuer: "https://idp.example", jwksURL: jwks.url(), audience: "transit-server", rolesClaim: "roles"},
		wasm:            newAuthzTestHost(t),
		jwksKeys:        map[string]crypto.PublicKey{},
		jwksMinInterval: 0,
	}
	defer st.Close()
	mux := http.NewServeMux()
	mux.HandleFunc("/edits", s.protectWrite(func(*http.Request) string { return "edits" }, s.handleEdits))
	base := httptest.NewServer(mux).URL

	now := time.Now().Unix()
	tok := signRS256(t, rsaKey, "k1", map[string]any{
		"iss": "https://idp.example", "aud": "transit-server",
		"sub": "admin-1", "roles": []string{"admin"}, "exp": now + 3600,
	})
	if code, body := bearerPost(t, base+"/edits", tok, `[]`); code != 403 || !strings.Contains(body, "no policy") {
		t.Fatalf("no policy must fail closed: %d %s", code, body)
	}
}

// The manifest pins the policy hash, but the blob is 404 even for an admin.
func TestAuthzPolicyBlobNeverServed(t *testing.T) {
	rsaKey, _ := rsa.GenerateKey(rand.Reader, 2048)
	jwks := newJWKSServer([]map[string]any{rsaJWK("k1", &rsaKey.PublicKey)})
	defer jwks.srv.Close()
	s, base := newAuthServer(t, jwks)

	policyHash := mustHead(t, s, "policy")

	resp, err := http.Get(base + "/blobs/" + policyHash)
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != 404 {
		t.Fatalf("policy blob anonymous: want 404, got %d", resp.StatusCode)
	}

	now := time.Now().Unix()
	admin := signRS256(t, rsaKey, "k1", map[string]any{
		"iss": "https://idp.example", "aud": "transit-server",
		"sub": "admin-1", "roles": []string{"admin"}, "exp": now + 3600,
	})
	req, _ := http.NewRequest("GET", base+"/blobs/"+policyHash, nil)
	req.Header.Set("Authorization", "Bearer "+admin)
	resp2, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	resp2.Body.Close()
	if resp2.StatusCode != 404 {
		t.Fatalf("policy blob as admin: want 404, got %d", resp2.StatusCode)
	}

	editsHash, err := s.store.PutBlob([]byte(editDoc("e1", "draft", 1, 100)), s.syncEnvelope(), "")
	if err != nil {
		t.Fatal(err)
	}
	resp3, err := http.Get(base + "/blobs/" + editsHash)
	if err != nil {
		t.Fatal(err)
	}
	body, _ := io.ReadAll(resp3.Body)
	resp3.Body.Close()
	if resp3.StatusCode != 200 || !strings.Contains(string(body), "[") {
		t.Fatalf("edits blob must stay public: %d %s", resp3.StatusCode, body)
	}
}

// The pointer log is authenticated because it carries writer identities.
// /logs/disruptions needs the audit rule for the same reason.
func TestAuthzLogWriterRedaction(t *testing.T) {
	rsaKey, _ := rsa.GenerateKey(rand.Reader, 2048)
	jwks := newJWKSServer([]map[string]any{rsaJWK("k1", &rsaKey.PublicKey)})
	defer jwks.srv.Close()
	s, base := newAuthServer(t, jwks)

	now := time.Now().Unix()
	mk := func(sub string, roles []string) string {
		return signRS256(t, rsaKey, "k1", map[string]any{
			"iss": "https://idp.example", "aud": "transit-server",
			"sub": sub, "roles": roles, "exp": now + 3600,
		})
	}
	bearerGet := func(url, token string) (int, string) {
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
		return resp.StatusCode, string(b)
	}

	if code, _ := bearerPost(t, base+"/edits", mk("dispatcher-7", []string{"dispatcher"}), editDoc("e1", "draft", 1, 100)); code != 200 {
		t.Fatalf("post: %d", code)
	}
	if _, _, err := s.store.CAS("edits", "", []byte(editDoc("e1", "draft", 1, 100)), s.syncEnvelope(), "", "dispatcher-7", 1); err != nil {
		t.Fatal(err)
	}
	entriesURL := base + "/logs/disruptions"
	if code, _ := bearerGet(entriesURL, ""); code != 401 {
		t.Fatalf("entries anonymous: want 401, got %d", code)
	}
	if code, _ := bearerGet(entriesURL, mk("dispatcher-7", []string{"dispatcher"})); code != 403 {
		t.Fatalf("entries for a reader without audit: want 403, got %d", code)
	}
	if code, body := bearerGet(entriesURL, mk("aud-1", []string{"auditor"})); code != 200 || !strings.Contains(body, "dispatcher-7") {
		t.Fatalf("auditor must see the entries' writers, got %d %s", code, body)
	}
	logURL := base + "/pointers/edits/log"

	if code, _ := bearerGet(logURL, ""); code != 401 {
		t.Fatalf("log anonymous: want 401, got %d", code)
	}
	code, body := bearerGet(logURL, mk("dispatcher-7", []string{"dispatcher"}))
	if code != 200 || strings.Contains(body, "dispatcher-7") {
		t.Fatalf("log for reader without audit: want 200 redacted, got %d %s", code, body)
	}
	if !strings.Contains(body, `"writer":""`) {
		t.Fatalf("writer must be redacted, got %s", body)
	}
	if _, body := bearerGet(logURL, mk("aud-1", []string{"auditor"})); !strings.Contains(body, "dispatcher-7") {
		t.Fatalf("auditor must see writers, got %s", body)
	}
	if _, body := bearerGet(logURL, mk("admin-1", []string{"admin"})); !strings.Contains(body, "dispatcher-7") {
		t.Fatalf("admin must see writers, got %s", body)
	}
	if code, _ := bearerGet(base+"/pointers/edits", ""); code != 200 {
		t.Fatalf("head must stay public: %d", code)
	}
	_ = s
}

// GET /permissions serves the caller's projection of the policy, never
// the policy document.
func TestAuthzPermissionsProjection(t *testing.T) {
	rsaKey, _ := rsa.GenerateKey(rand.Reader, 2048)
	jwks := newJWKSServer([]map[string]any{rsaJWK("k1", &rsaKey.PublicKey)})
	defer jwks.srv.Close()
	s, _ := newAuthServer(t, jwks)

	mux := http.NewServeMux()
	mux.HandleFunc("/permissions", s.requireAuth(s.handlePermissions))
	mux.HandleFunc("/blobs/", s.handleBlob)
	permBase := httptest.NewServer(mux).URL

	now := time.Now().Unix()
	mk := func(sub string, roles []string) string {
		return signRS256(t, rsaKey, "k1", map[string]any{
			"iss": "https://idp.example", "aud": "transit-server",
			"sub": sub, "roles": roles, "exp": now + 3600,
		})
	}
	get := func(token string) (int, string) {
		req, _ := http.NewRequest("GET", permBase+"/permissions", nil)
		if token != "" {
			req.Header.Set("Authorization", "Bearer "+token)
		}
		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		defer resp.Body.Close()
		b, _ := io.ReadAll(resp.Body)
		return resp.StatusCode, string(b)
	}

	if code, _ := get(""); code != 401 {
		t.Fatalf("permissions anonymous: want 401, got %d", code)
	}
	code, body := get(mk("dispatcher-7", []string{"dispatcher"}))
	if code != 200 {
		t.Fatalf("permissions: %d %s", code, body)
	}
	if !strings.Contains(body, `"edits"`) || !strings.Contains(body, `"write"`) || !strings.Contains(body, `"dispatcher"`) {
		t.Fatalf("own capabilities missing: %s", body)
	}
	for _, leak := range []string{"admin", "auditor", "auditors", "admin-all", "rules"} {
		if strings.Contains(body, leak) {
			t.Fatalf("policy internals leaked to the client (%q): %s", leak, body)
		}
	}
	_, body = get(mk("aud-1", []string{"auditor"}))
	if !strings.Contains(body, `"audit"`) || strings.Contains(body, "edits") {
		t.Fatalf("auditor projection: %s", body)
	}
	policyHash := mustHead(t, s, "policy")
	req, _ := http.NewRequest("GET", permBase+"/blobs/"+policyHash, nil)
	req.Header.Set("Authorization", "Bearer "+mk("admin-1", []string{"admin"}))
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != 404 {
		t.Fatalf("policy blob must stay 404 for admin too: %d", resp.StatusCode)
	}
}

func mustHead(t *testing.T, s *server, pointer string) string {
	t.Helper()
	head, found, err := s.store.Head(pointer)
	if err != nil || !found {
		t.Fatalf("head %s: %v %v", pointer, found, err)
	}
	return head.Hash
}

// D-158 (-oidc-name-claim): the name is the configured claim, a nested
// path, or the sub when the token has none.
func TestAuthzNameIsTheConfiguredClaim(t *testing.T) {
	rsaKey, _ := rsa.GenerateKey(rand.Reader, 2048)
	jwks := newJWKSServer([]map[string]any{rsaJWK("k1", &rsaKey.PublicKey)})
	defer jwks.srv.Close()
	s, _ := newAuthServer(t, jwks)
	now := time.Now().Unix()
	tok := signRS256(t, rsaKey, "k1", map[string]any{
		"iss": "https://idp.example", "aud": "transit-server", "sub": "00u1abc", "exp": now + 3600,
		"name": "Rosa Ortiz", "upn": "rosa.ortiz@agency.example", "ext": map[string]any{"display": "R. Ortiz"},
	})
	for claim, want := range map[string]string{"": "Rosa Ortiz", "name": "Rosa Ortiz", "upn": "rosa.ortiz@agency.example", "ext.display": "R. Ortiz", "preferred_username": "00u1abc"} {
		s.authz.cfg.nameClaim = claim
		if claim == "" {
			s.authz.cfg.nameClaim = "name" // loadAuthz's default
		}
		id, err := s.authz.verifyJWT(context.Background(), tok)
		if err != nil {
			t.Fatal(err)
		}
		if id.Name != want {
			t.Fatalf("claim %q: name %q, want %q", claim, id.Name, want)
		}
	}
}
