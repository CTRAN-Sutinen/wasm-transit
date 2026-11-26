// Package authz is sign-in: the host verifies the bearer JWT and the
// cap-authz module decides (docs/architecture/server.md). Every write is
// denied when there is no valid token or no matching rule. A verified
// subject becomes the store log's `writer`.
package authz

import (
	"context"
	"crypto"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rsa"
	"crypto/sha256"
	"crypto/x509"
	"encoding/base64"
	"encoding/json"
	"encoding/pem"
	"errors"
	"fmt"
	"io"
	"log"
	"math/big"
	"net/http"
	"os"
	"strings"
	"sync"
	"time"

	"rustwasm/server/httpx"
	"rustwasm/server/source"
	"rustwasm/server/store"
	"rustwasm/server/wasmhost"
)

type Config struct {
	Issuer     string
	JWKSURL    string
	Audience   string
	KeyFile    string // pinned PEM public key (used when JWKSURL is empty, or as bootstrap)
	RolesClaim string // dotted claim path, default "roles"
	NameClaim  string // dotted claim path to the person's name, default "name"
	ClientID   string // the pages' public client (PKCE), published in the manifest
	Scope      string // the scopes a page asks for
}

// A nil Auth means auth is off: the dev default, which writes
// unauthenticated.
type Auth struct {
	Cfg Config
	// Wasm is cap-authz, in an instance of its own: a trap in another
	// capability never takes the fail-closed check with it.
	Wasm   *wasmhost.Host
	pinned crypto.PublicKey

	jwksMu          sync.Mutex
	jwksKeys        map[string]crypto.PublicKey // by kid
	jwksFetched     time.Time
	jwksMinInterval time.Duration // refetch rate limit; Load sets 30s, tests 0

	// policyBytes is the policy when there is no store. With -store, each
	// request reads the `policy` pointer instead.
	policyMu    sync.RWMutex
	policyBytes []byte
}

// New is an Auth that fetches the JWKS at most once each minInterval.
func New(cfg Config, wasm *wasmhost.Host, minInterval time.Duration) *Auth {
	return &Auth{Cfg: cfg, Wasm: wasm, jwksKeys: map[string]crypto.PublicKey{}, jwksMinInterval: minInterval}
}

func Load(ctx context.Context, cfg Config, module *wasmhost.Instance, policyFile string) (*Auth, error) {
	inst, err := module.Fresh(ctx)
	if err != nil {
		return nil, fmt.Errorf("authz instance: %w", err)
	}
	a := New(cfg, wasmhost.NewHeld("authz", inst).As("authz"), 30*time.Second)
	if a.Cfg.NameClaim == "" {
		a.Cfg.NameClaim = "name"
	}
	if a.Cfg.RolesClaim == "" {
		a.Cfg.RolesClaim = "roles"
	}
	if cfg.KeyFile != "" {
		pemBytes, err := os.ReadFile(cfg.KeyFile)
		if err != nil {
			return nil, fmt.Errorf("oidc key: %w", err)
		}
		key, err := parsePublicPEM(pemBytes)
		if err != nil {
			return nil, fmt.Errorf("oidc key: %w", err)
		}
		a.pinned = key
	}
	if policyFile != "" {
		b, _, err := source.New(policyFile, "", "", nil).Read()
		if err != nil {
			return nil, fmt.Errorf("policy: %w", err)
		}
		a.policyBytes = b
	}
	return a, nil
}

func parsePublicPEM(b []byte) (crypto.PublicKey, error) {
	block, _ := pem.Decode(b)
	if block == nil {
		return nil, errors.New("no PEM block")
	}
	if key, err := x509.ParsePKCS1PublicKey(block.Bytes); err == nil {
		return key, nil
	}
	key, err := x509.ParsePKIXPublicKey(block.Bytes)
	if err != nil {
		return nil, err
	}
	switch k := key.(type) {
	case *rsa.PublicKey, *ecdsa.PublicKey:
		return k, nil
	}
	return nil, fmt.Errorf("unsupported key type %T", key)
}

func b64url(s string) ([]byte, error) {
	return base64.RawURLEncoding.DecodeString(s)
}

type jwk struct {
	Kid string `json:"kid"`
	Kty string `json:"kty"`
	Alg string `json:"alg"`
	Use string `json:"use"`
	N   string `json:"n"`   // RSA
	E   string `json:"e"`   // RSA
	Crv string `json:"crv"` // EC
	X   string `json:"x"`   // EC
	Y   string `json:"y"`   // EC
}

func (k jwk) publicKey() (crypto.PublicKey, error) {
	switch k.Kty {
	case "RSA":
		nBytes, err := b64url(k.N)
		if err != nil {
			return nil, err
		}
		eBytes, err := b64url(k.E)
		if err != nil {
			return nil, err
		}
		e := 0
		for _, b := range eBytes {
			e = e<<8 | int(b)
		}
		return &rsa.PublicKey{N: new(big.Int).SetBytes(nBytes), E: e}, nil
	case "EC":
		if k.Crv != "P-256" {
			return nil, fmt.Errorf("unsupported crv %q", k.Crv)
		}
		xBytes, err := b64url(k.X)
		if err != nil {
			return nil, err
		}
		yBytes, err := b64url(k.Y)
		if err != nil {
			return nil, err
		}
		return &ecdsa.PublicKey{
			Curve: elliptic.P256(),
			X:     new(big.Int).SetBytes(xBytes),
			Y:     new(big.Int).SetBytes(yBytes),
		}, nil
	}
	return nil, fmt.Errorf("unsupported kty %q", k.Kty)
}

var jwksHTTP = &http.Client{Transport: httpx.Outbound, Timeout: 10 * time.Second}

// keyFor fetches the JWKS on a miss, at most once per jwksMinInterval.
func (a *Auth) keyFor(ctx context.Context, kid string) (crypto.PublicKey, error) {
	if a.pinned != nil {
		return a.pinned, nil
	}
	a.jwksMu.Lock()
	key, ok := a.jwksKeys[kid]
	canFetch := time.Since(a.jwksFetched) > a.jwksMinInterval
	a.jwksMu.Unlock()
	if ok {
		return key, nil
	}
	if a.Cfg.JWKSURL == "" {
		return nil, errors.New("no JWKS URL and no pinned key configured")
	}
	if !canFetch {
		return nil, fmt.Errorf("unknown kid %q (JWKS refetch rate-limited)", kid)
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, a.Cfg.JWKSURL, nil)
	if err != nil {
		return nil, err
	}
	resp, err := jwksHTTP.Do(req)
	if err != nil {
		return nil, fmt.Errorf("jwks fetch: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != 200 {
		return nil, fmt.Errorf("jwks fetch: %s", resp.Status)
	}
	body, err := io.ReadAll(io.LimitReader(resp.Body, 4<<20))
	if err != nil {
		return nil, err
	}
	var set struct {
		Keys []jwk `json:"keys"`
	}
	if err := json.Unmarshal(body, &set); err != nil {
		return nil, fmt.Errorf("jwks parse: %w", err)
	}
	a.jwksMu.Lock()
	a.jwksFetched = time.Now()
	for _, k := range set.Keys {
		if k.Use != "" && k.Use != "sig" {
			continue
		}
		if key, err := k.publicKey(); err == nil {
			a.jwksKeys[k.Kid] = key
		}
	}
	key, ok = a.jwksKeys[kid]
	a.jwksMu.Unlock()
	if !ok {
		return nil, fmt.Errorf("kid %q not in the JWKS", kid)
	}
	return key, nil
}

type Identity struct {
	Sub   string   `json:"sub"`
	Roles []string `json:"roles"`
	// Name is the configured name claim (-oidc-name-claim), else the sub.
	// A record names the person by it (a survey's surveyor).
	Name string `json:"-"`
}

const clockLeeway = 60 * time.Second

// Verify accepts RS256 and ES256 only, so `none` and HS* tokens fail.
func (a *Auth) Verify(ctx context.Context, token string) (*Identity, error) {
	parts := strings.Split(token, ".")
	if len(parts) != 3 {
		return nil, errors.New("not a JWT")
	}
	var header struct {
		Alg string `json:"alg"`
		Kid string `json:"kid"`
	}
	hBytes, err := b64url(parts[0])
	if err != nil || json.Unmarshal(hBytes, &header) != nil {
		return nil, errors.New("bad header")
	}
	if header.Alg != "RS256" && header.Alg != "ES256" {
		return nil, fmt.Errorf("alg %q rejected (RS256/ES256 only)", header.Alg)
	}
	key, err := a.keyFor(ctx, header.Kid)
	if err != nil {
		return nil, err
	}
	sig, err := b64url(parts[2])
	if err != nil {
		return nil, errors.New("bad signature encoding")
	}
	signingInput := parts[0] + "." + parts[1]
	digest := sha256.Sum256([]byte(signingInput))
	switch k := key.(type) {
	case *rsa.PublicKey:
		if header.Alg != "RS256" {
			return nil, fmt.Errorf("alg %q with an RSA key", header.Alg)
		}
		if err := rsa.VerifyPKCS1v15(k, crypto.SHA256, digest[:], sig); err != nil {
			return nil, errors.New("bad signature")
		}
	case *ecdsa.PublicKey:
		if header.Alg != "ES256" {
			return nil, fmt.Errorf("alg %q with an EC key", header.Alg)
		}
		if len(sig) != 64 {
			return nil, errors.New("bad ES256 signature length")
		}
		r := new(big.Int).SetBytes(sig[:32])
		s := new(big.Int).SetBytes(sig[32:])
		if !ecdsa.Verify(k, digest[:], r, s) {
			return nil, errors.New("bad signature")
		}
	default:
		return nil, fmt.Errorf("unsupported key type %T", key)
	}

	pBytes, err := b64url(parts[1])
	if err != nil {
		return nil, errors.New("bad payload encoding")
	}
	var claims map[string]any
	if err := json.Unmarshal(pBytes, &claims); err != nil {
		return nil, errors.New("bad claims")
	}
	now := time.Now()
	if a.Cfg.Issuer != "" {
		if claims["iss"] != a.Cfg.Issuer {
			return nil, fmt.Errorf("iss %v != %q", claims["iss"], a.Cfg.Issuer)
		}
	}
	if a.Cfg.Audience != "" {
		audOK := claims["aud"] == a.Cfg.Audience
		if !audOK {
			if arr, ok := claims["aud"].([]any); ok {
				for _, v := range arr {
					if v == a.Cfg.Audience {
						audOK = true
					}
				}
			}
		}
		if !audOK {
			return nil, fmt.Errorf("aud %v does not contain %q", claims["aud"], a.Cfg.Audience)
		}
	}
	if exp, ok := claims["exp"].(float64); ok {
		if now.After(time.Unix(int64(exp), 0).Add(clockLeeway)) {
			return nil, errors.New("token expired")
		}
	}
	if nbf, ok := claims["nbf"].(float64); ok {
		if now.Add(clockLeeway).Before(time.Unix(int64(nbf), 0)) {
			return nil, errors.New("token not yet valid")
		}
	}
	sub, _ := claims["sub"].(string)
	if sub == "" {
		return nil, errors.New("no sub")
	}
	name := stringAtClaim(claims, a.Cfg.NameClaim)
	if name == "" {
		name = sub
	}
	return &Identity{Sub: sub, Roles: rolesAtClaim(claims, a.Cfg.RolesClaim), Name: name}, nil
}

func stringAtClaim(claims map[string]any, path string) string {
	cur := any(claims)
	for _, seg := range strings.Split(path, ".") {
		m, ok := cur.(map[string]any)
		if !ok {
			return ""
		}
		if cur, ok = m[seg]; !ok {
			return ""
		}
	}
	s, _ := cur.(string)
	return strings.TrimSpace(s)
}

// A missing or mistyped leaf gives no roles, and then no rule matches.
func rolesAtClaim(claims map[string]any, path string) []string {
	cur := any(claims)
	for _, seg := range strings.Split(path, ".") {
		m, ok := cur.(map[string]any)
		if !ok {
			return nil
		}
		cur, ok = m[seg]
		if !ok {
			return nil
		}
	}
	arr, ok := cur.([]any)
	if !ok {
		return nil
	}
	roles := make([]string, 0, len(arr))
	for _, v := range arr {
		if s, ok := v.(string); ok {
			roles = append(roles, s)
		}
	}
	return roles
}

type ctxKey string

const (
	identityKey ctxKey = "identity" // the verified *Identity (sub + roles)
	auditKey    ctxKey = "audit"    // may the caller see writer identities in the log?
)

// CanAudit says whether the caller may see the writers in a log; true when
// no check ran.
func CanAudit(r *http.Request) bool {
	if v, ok := r.Context().Value(auditKey).(bool); ok {
		return v
	}
	return true
}

// A Gate checks a request against the policy. With a nil Auth, sign-in is
// off and every request passes.
type Gate struct {
	Auth *Auth
	// Store holds the policy pointer. With no store, the policy is the
	// file that Load read.
	Store func() *store.Store
	// LogRule is the write rule of the log that /logs/<name> names, and
	// whether the server keeps that log.
	LogRule func(name string) (rule string, kept bool)
}

func (g *Gate) store() *store.Store { return g.Store() }

func (g *Gate) PolicyDocument() []byte {
	if g.store() != nil {
		head, found, err := g.store().Head("policy")
		if err != nil || !found {
			return nil
		}
		b, _, err := g.store().GetBlob(head.Hash)
		if err != nil {
			return nil
		}
		return b
	}
	if g.Auth == nil {
		return nil
	}
	g.Auth.policyMu.RLock()
	defer g.Auth.policyMu.RUnlock()
	return g.Auth.policyBytes
}

// authorize denies on any error, so it fails closed.
func (g *Gate) authorize(r *http.Request, id *Identity, action, pointer string) (bool, string) {
	policy := g.PolicyDocument()
	if len(policy) == 0 {
		return false, "no policy document (fail-closed)"
	}
	var resp struct {
		Decision struct {
			Allow       bool    `json:"allow"`
			MatchedRule *string `json:"matched_rule"`
			Reason      string  `json:"reason"`
		} `json:"decision"`
	}
	err := g.Auth.Wasm.Invoke(r.Context(), map[string]any{
		"op":       "authorize",
		"identity": map[string]any{"sub": id.Sub, "roles": id.Roles},
		"action":   action,
		"pointer":  pointer,
		"policy":   json.RawMessage(policy),
	}, &resp)
	if err != nil {
		return false, "authorize op failed (fail-closed): " + err.Error()
	}
	if !resp.Decision.Allow {
		return false, resp.Decision.Reason
	}
	return true, resp.Decision.Reason
}

// ProtectWrite leaves reads public, except the pointer log, which
// carries writer identities. That read needs read on the pointer, and
// the writers stay redacted unless the identity holds an "audit" rule.
// pointerOf names the pointer. With auth disabled, everything passes.
func (g *Gate) ProtectWrite(pointerOf func(*http.Request) string, next http.HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		isLog := r.Method == http.MethodGet && strings.HasSuffix(r.URL.Path, "/log")
		if g.Auth == nil || ((r.Method == http.MethodGet || r.Method == http.MethodHead) && !isLog) {
			next(w, r)
			return
		}
		auth := r.Header.Get("Authorization")
		if !strings.HasPrefix(auth, "Bearer ") {
			http.Error(w, "missing bearer token", http.StatusUnauthorized)
			return
		}
		id, err := g.Auth.Verify(r.Context(), strings.TrimPrefix(auth, "Bearer "))
		if err != nil {
			http.Error(w, "invalid token: "+err.Error(), http.StatusUnauthorized)
			return
		}
		pointer := pointerOf(r)
		ctx := context.WithValue(r.Context(), identityKey, id)
		ctx = httpx.WithWriter(httpx.WithName(ctx, id.Name), id.Sub)
		if isLog {
			// An "audit" rule implies read. The log's `writer` fields are
			// shown to an audit rule and redacted otherwise (D-164).
			allowRead, reason := g.authorize(r, id, "read", pointer)
			canAudit, _ := g.authorize(r, id, "audit", pointer)
			if !allowRead && !canAudit {
				log.Printf("authz deny: sub=%s read %s: %s", id.Sub, pointer, reason)
				http.Error(w, "forbidden: "+reason, http.StatusForbidden)
				return
			}
			ctx = context.WithValue(ctx, auditKey, canAudit)
			next(w, r.WithContext(ctx))
			return
		}
		allow, reason := g.authorize(r, id, "write", pointer)
		if !allow {
			log.Printf("authz deny: sub=%s write %s: %s", id.Sub, pointer, reason)
			http.Error(w, "forbidden: "+reason, http.StatusForbidden)
			return
		}
		next(w, r.WithContext(ctx))
	}
}

func (g *Gate) logPointer(path string) string {
	name, _, _ := strings.Cut(strings.TrimPrefix(path, "/logs/"), "/")
	if rule, _ := g.LogRule(name); rule != "" {
		return rule
	}
	return name
}

func (g *Gate) MayWrite(r *http.Request, pointer string) (bool, string) {
	if g.Auth == nil {
		return true, ""
	}
	id, _ := r.Context().Value(identityKey).(*Identity)
	if id == nil {
		return false, "no verified identity"
	}
	return g.authorize(r, id, "write", pointer)
}

// ProtectLog needs the "audit" grant on the log's pointer for every
// call, a read too. An entry names its writer inside its bytes, so the
// server cannot redact it. An intent write (POST /logs/<log>) needs only
// a valid token here, and the write rule of each document it writes
// (server/logs).
func (g *Gate) ProtectLog(next http.HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if g.Auth == nil {
			next(w, r)
			return
		}
		auth := r.Header.Get("Authorization")
		if !strings.HasPrefix(auth, "Bearer ") {
			http.Error(w, "missing bearer token", http.StatusUnauthorized)
			return
		}
		id, err := g.Auth.Verify(r.Context(), strings.TrimPrefix(auth, "Bearer "))
		if err != nil {
			http.Error(w, "invalid token: "+err.Error(), http.StatusUnauthorized)
			return
		}
		ctx := context.WithValue(r.Context(), identityKey, id)
		ctx = httpx.WithWriter(httpx.WithName(ctx, id.Name), id.Sub)
		if r.Method == http.MethodPost && g.isIntentWrite(r.URL.Path) {
			next(w, r.WithContext(ctx))
			return
		}
		if allow, reason := g.authorize(r, id, "audit", g.logPointer(r.URL.Path)); !allow {
			log.Printf("authz deny: sub=%s audit %s: %s", id.Sub, r.URL.Path, reason)
			http.Error(w, "forbidden: "+reason, http.StatusForbidden)
			return
		}
		ctx = context.WithValue(ctx, auditKey, true)
		next(w, r.WithContext(ctx))
	}
}

func (g *Gate) isIntentWrite(path string) bool {
	name, ok := strings.CutPrefix(path, "/logs/")
	if !ok {
		return false
	}
	_, kept := g.LogRule(name)
	return kept
}

func (g *Gate) RequireAuth(next http.HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if g.Auth == nil {
			http.Error(w, "auth is not configured", http.StatusNotFound)
			return
		}
		auth := r.Header.Get("Authorization")
		if !strings.HasPrefix(auth, "Bearer ") {
			http.Error(w, "missing bearer token", http.StatusUnauthorized)
			return
		}
		id, err := g.Auth.Verify(r.Context(), strings.TrimPrefix(auth, "Bearer "))
		if err != nil {
			http.Error(w, "invalid token: "+err.Error(), http.StatusUnauthorized)
			return
		}
		ctx := context.WithValue(r.Context(), identityKey, id)
		ctx = httpx.WithWriter(httpx.WithName(ctx, id.Name), id.Sub)
		next(w, r.WithContext(ctx))
	}
}

// HandlePermissions is the only client-readable form of the policy. The
// policy document itself is never served: /blobs answers 404 for kind
// "policy".
func (g *Gate) HandlePermissions(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "use GET", http.StatusMethodNotAllowed)
		return
	}
	var id *Identity
	if v := r.Context().Value(identityKey); v != nil {
		id = v.(*Identity)
	}
	sub := ""
	var roles []string
	if id != nil {
		sub = id.Sub
		roles = id.Roles
	}
	policy := g.PolicyDocument()
	var resp struct {
		Effective struct {
			Roles        []string `json:"roles"`
			Capabilities []struct {
				Pointer string   `json:"pointer"`
				Actions []string `json:"actions"`
			} `json:"capabilities"`
		} `json:"effective"`
	}
	rawPolicy := json.RawMessage(policy)
	if len(rawPolicy) == 0 {
		rawPolicy = json.RawMessage(`{"schema_version":1,"rules":[]}`)
	}
	err := g.Auth.Wasm.Invoke(r.Context(), map[string]any{
		"op":       "effective",
		"identity": map[string]any{"sub": sub, "roles": roles},
		"policy":   rawPolicy,
	}, &resp)
	if err != nil {
		http.Error(w, "effective op failed", http.StatusInternalServerError)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(resp.Effective)
}

// SeedPolicy commits the -policy source as the `policy` pointer
// when it does not exist yet. Later changes are ordinary CAS
// commits.
func (g *Gate) SeedPolicy(policyFile string) {
	if g.store() == nil || policyFile == "" {
		return
	}
	if _, found, err := g.store().Head("policy"); err == nil && found {
		return
	}
	raw, _, err := source.New(policyFile, "", "", g.store()).Read()
	if err != nil || len(raw) == 0 {
		log.Printf("seed policy: %v", err)
		return
	}
	env := store.Envelope{Kind: "policy", SchemaVersion: 1, ProducedBy: "server"}
	if _, _, err := g.store().CAS("policy", "", raw, env, "", "bootstrap:"+policyFile, time.Now().UnixMilli()); err != nil {
		log.Printf("seed policy: %v", err)
		return
	}
	log.Printf("policy seeded into the store from %s", policyFile)
}

// ProtectSignedIn requires any valid token and no policy rule.
func (g *Gate) ProtectSignedIn(next http.HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if g.Auth == nil {
			next(w, r)
			return
		}
		auth := r.Header.Get("Authorization")
		if !strings.HasPrefix(auth, "Bearer ") {
			http.Error(w, "sign in to see attachments", http.StatusUnauthorized)
			return
		}
		id, err := g.Auth.Verify(r.Context(), strings.TrimPrefix(auth, "Bearer "))
		if err != nil {
			http.Error(w, "invalid token: "+err.Error(), http.StatusUnauthorized)
			return
		}
		next(w, r.WithContext(httpx.WithName(context.WithValue(r.Context(), identityKey, id), id.Name)))
	}
}
