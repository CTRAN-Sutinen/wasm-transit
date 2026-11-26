package store

import (
	"context"
	"crypto"
	"crypto/rand"
	"crypto/rsa"
	"crypto/sha256"
	"crypto/x509"
	"encoding/base64"
	"encoding/json"
	"encoding/pem"
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"os"
	"strings"
	"sync"
	"time"
)

// The GCS credential sources that DEPLOYMENTS-SERVER.md names, and no
// others: a service account's key file at GOOGLE_APPLICATION_CREDENTIALS
// (a JWT signed with its key, exchanged for an access token), else the
// metadata server of the machine or pod (GCE_METADATA_HOST overrides
// its address).

const gcsScope = "https://www.googleapis.com/auth/devstorage.read_write"

type gcpToken struct {
	value   string
	expires time.Time
}

type gcpSource interface {
	token(ctx context.Context) (gcpToken, error)
}

type gcpTokenCache struct {
	src  gcpSource
	mu   sync.Mutex
	have gcpToken
	now  func() time.Time
}

func (c *gcpTokenCache) get(ctx context.Context) (string, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.have.value != "" && c.now().Add(awsCredSlack).Before(c.have.expires) {
		return c.have.value, nil
	}
	t, err := c.src.token(ctx)
	if err != nil {
		return "", fmt.Errorf("gcs credentials: %w", err)
	}
	c.have = t
	return t.value, nil
}

func gcpSourceFromEnv(httpc *http.Client, getenv func(string) string) (gcpSource, error) {
	if path := getenv("GOOGLE_APPLICATION_CREDENTIALS"); path != "" {
		raw, err := os.ReadFile(path)
		if err != nil {
			return nil, fmt.Errorf("GOOGLE_APPLICATION_CREDENTIALS: %w", err)
		}
		return parseServiceAccount(httpc, raw)
	}
	host := getenv("GCE_METADATA_HOST")
	if host == "" {
		host = "metadata.google.internal"
	}
	return metadataGCP{httpc: httpc, base: "http://" + host}, nil
}

type serviceAccountGCP struct {
	httpc    *http.Client
	email    string
	keyID    string
	key      *rsa.PrivateKey
	tokenURI string
	now      func() time.Time
}

func parseServiceAccount(httpc *http.Client, raw []byte) (*serviceAccountGCP, error) {
	var f struct {
		Type         string `json:"type"`
		ClientEmail  string `json:"client_email"`
		PrivateKeyID string `json:"private_key_id"`
		PrivateKey   string `json:"private_key"`
		TokenURI     string `json:"token_uri"`
	}
	if err := json.Unmarshal(raw, &f); err != nil {
		return nil, fmt.Errorf("GOOGLE_APPLICATION_CREDENTIALS: %w", err)
	}
	if f.Type != "service_account" {
		return nil, fmt.Errorf("GOOGLE_APPLICATION_CREDENTIALS: type %q: only a service account's key file is read", f.Type)
	}
	block, _ := pem.Decode([]byte(f.PrivateKey))
	if block == nil {
		return nil, errors.New("GOOGLE_APPLICATION_CREDENTIALS: private_key is not PEM")
	}
	var key *rsa.PrivateKey
	if k, err := x509.ParsePKCS8PrivateKey(block.Bytes); err == nil {
		rk, ok := k.(*rsa.PrivateKey)
		if !ok {
			return nil, errors.New("GOOGLE_APPLICATION_CREDENTIALS: private_key is not RSA")
		}
		key = rk
	} else if rk, err2 := x509.ParsePKCS1PrivateKey(block.Bytes); err2 == nil {
		key = rk
	} else {
		return nil, fmt.Errorf("GOOGLE_APPLICATION_CREDENTIALS: private_key: %w", err)
	}
	if f.TokenURI == "" {
		f.TokenURI = "https://oauth2.googleapis.com/token"
	}
	return &serviceAccountGCP{httpc: httpc, email: f.ClientEmail, keyID: f.PrivateKeyID,
		key: key, tokenURI: f.TokenURI, now: time.Now}, nil
}

func b64url(b []byte) string { return base64.RawURLEncoding.EncodeToString(b) }

// assertion is the RS256 JWT that the token endpoint exchanges.
func (s *serviceAccountGCP) assertion(now time.Time) (string, error) {
	hdr, _ := json.Marshal(map[string]string{"alg": "RS256", "typ": "JWT", "kid": s.keyID})
	claims, _ := json.Marshal(map[string]any{
		"iss": s.email, "scope": gcsScope, "aud": s.tokenURI,
		"iat": now.Unix(), "exp": now.Add(time.Hour).Unix(),
	})
	signing := b64url(hdr) + "." + b64url(claims)
	h := sha256.Sum256([]byte(signing))
	sig, err := rsa.SignPKCS1v15(rand.Reader, s.key, crypto.SHA256, h[:])
	if err != nil {
		return "", err
	}
	return signing + "." + b64url(sig), nil
}

type gcpTokenJSON struct {
	AccessToken string `json:"access_token"`
	ExpiresIn   int64  `json:"expires_in"`
}

func (j gcpTokenJSON) token(now time.Time, what string) (gcpToken, error) {
	if j.AccessToken == "" {
		return gcpToken{}, fmt.Errorf("%s answer holds no token", what)
	}
	return gcpToken{j.AccessToken, now.Add(time.Duration(j.ExpiresIn) * time.Second)}, nil
}

func (s *serviceAccountGCP) token(ctx context.Context) (gcpToken, error) {
	now := s.now()
	jwt, err := s.assertion(now)
	if err != nil {
		return gcpToken{}, err
	}
	form := url.Values{"grant_type": {"urn:ietf:params:oauth:grant-type:jwt-bearer"}, "assertion": {jwt}}
	req, err := http.NewRequestWithContext(ctx, "POST", s.tokenURI, strings.NewReader(form.Encode()))
	if err != nil {
		return gcpToken{}, err
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	resp, err := s.httpc.Do(req)
	if err != nil {
		return gcpToken{}, err
	}
	body, err := readBody(resp, "token endpoint")
	if err != nil {
		return gcpToken{}, err
	}
	var j gcpTokenJSON
	if err := json.Unmarshal(body, &j); err != nil {
		return gcpToken{}, fmt.Errorf("token endpoint: %w", err)
	}
	return j.token(now, "token endpoint")
}

type metadataGCP struct {
	httpc *http.Client
	base  string
}

func (m metadataGCP) token(ctx context.Context) (gcpToken, error) {
	now := time.Now()
	req, err := http.NewRequestWithContext(ctx, "GET",
		m.base+"/computeMetadata/v1/instance/service-accounts/default/token", nil)
	if err != nil {
		return gcpToken{}, err
	}
	req.Header.Set("Metadata-Flavor", "Google")
	resp, err := m.httpc.Do(req)
	if err != nil {
		return gcpToken{}, fmt.Errorf("metadata server (GOOGLE_APPLICATION_CREDENTIALS is not set): %w", err)
	}
	body, err := readBody(resp, "metadata server")
	if err != nil {
		return gcpToken{}, err
	}
	var j gcpTokenJSON
	if err := json.Unmarshal(body, &j); err != nil {
		return gcpToken{}, fmt.Errorf("metadata server: %w", err)
	}
	return j.token(now, "metadata server")
}
