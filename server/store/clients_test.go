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
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

// The examples in AWS's S3 SigV4 reference
// (sig-v4-header-based-auth.html), with its published signatures.
func TestSigV4AWSExamples(t *testing.T) {
	c := awsCreds{AccessKeyID: "AKIAIOSFODNN7EXAMPLE", SecretAccessKey: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"}
	at := time.Date(2013, 5, 24, 0, 0, 0, 0, time.UTC)
	cases := []struct {
		name, method, url string
		hdr               map[string]string
		payload           string
		want              string
	}{
		{"GET object", "GET", "https://examplebucket.s3.amazonaws.com/test.txt",
			map[string]string{"Range": "bytes=0-9"}, emptySHA256,
			"f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"},
		{"GET bucket lifecycle", "GET", "https://examplebucket.s3.amazonaws.com/?lifecycle=",
			nil, emptySHA256,
			"fea454ca298b7da1c68078a5d1bdbfbbe0d65c699e0f91ac7a200a0136783543"},
		{"GET bucket list", "GET", "https://examplebucket.s3.amazonaws.com/?max-keys=2&prefix=J",
			nil, emptySHA256,
			"34b48302e7b5fa45bde8084f4b7868a86f0a534bc59db6670ed5711ef69dc6f7"},
		{"PUT object", "PUT", "https://examplebucket.s3.amazonaws.com/test$file.text",
			map[string]string{"Date": "Fri, 24 May 2013 00:00:00 GMT", "X-Amz-Storage-Class": "REDUCED_REDUNDANCY"},
			sha256Hex([]byte("Welcome to Amazon S3.")),
			"98ad721746da40c64f1a55b78f14c238d841ea1380cd77a1b5971af0ece108bd"},
	}
	for _, tc := range cases {
		req, _ := http.NewRequest(tc.method, tc.url, nil)
		for k, v := range tc.hdr {
			req.Header.Set(k, v)
		}
		signV4(req, tc.payload, c, "us-east-1", "s3", at)
		got := req.Header.Get("Authorization")
		if !strings.HasSuffix(got, "Signature="+tc.want) {
			t.Errorf("%s: %s", tc.name, got)
		}
	}
}

func TestAWSEscape(t *testing.T) {
	if got := awsEscapePath("/b/logs/a b:c~d+e"); got != "/b/logs/a%20b%3Ac~d%2Be" {
		t.Errorf("path: %s", got)
	}
	if got := awsCanonicalQuery(url.Values{"start-after": {"p/x"}, "list-type": {"2"}}); got != "list-type=2&start-after=p%2Fx" {
		t.Errorf("query: %s", got)
	}
}

func envOf(m map[string]string) func(string) string { return func(k string) string { return m[k] } }

func TestAWSSourceStatic(t *testing.T) {
	e := defaultAWSEnv()
	e.getenv = envOf(map[string]string{"AWS_ACCESS_KEY_ID": "id", "AWS_SECRET_ACCESS_KEY": "sk", "AWS_SESSION_TOKEN": "st"})
	c, err := e.source("us-west-2").fetch(context.Background())
	if err != nil || c != (awsCreds{AccessKeyID: "id", SecretAccessKey: "sk", SessionToken: "st"}) {
		t.Fatalf("%+v %v", c, err)
	}
}

func TestAWSSourceWebIdentity(t *testing.T) {
	exp := time.Date(2030, 1, 1, 0, 0, 0, 0, time.UTC)
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		r.ParseForm()
		if r.Form.Get("Action") != "AssumeRoleWithWebIdentity" || r.Form.Get("RoleArn") != "arn:role" ||
			r.Form.Get("WebIdentityToken") != "jwt-token" || r.Form.Get("RoleSessionName") != "wasm-transit" {
			http.Error(w, "bad form "+r.Form.Encode(), 400)
			return
		}
		w.Write([]byte(`<AssumeRoleWithWebIdentityResponse xmlns="https://sts.amazonaws.com/doc/2011-06-15/">
<AssumeRoleWithWebIdentityResult><Credentials><AccessKeyId>ASIA</AccessKeyId>
<SecretAccessKey>sk</SecretAccessKey><SessionToken>st</SessionToken>
<Expiration>2030-01-01T00:00:00Z</Expiration></Credentials></AssumeRoleWithWebIdentityResult>
</AssumeRoleWithWebIdentityResponse>`))
	}))
	defer srv.Close()
	tok := filepath.Join(t.TempDir(), "token")
	os.WriteFile(tok, []byte("jwt-token\n"), 0o600)
	e := defaultAWSEnv()
	e.getenv = envOf(map[string]string{"AWS_WEB_IDENTITY_TOKEN_FILE": tok, "AWS_ROLE_ARN": "arn:role", "AWS_ENDPOINT_URL_STS": srv.URL})
	c, err := e.source("us-west-2").fetch(context.Background())
	if err != nil || c != (awsCreds{"ASIA", "sk", "st", exp}) {
		t.Fatalf("%+v %v", c, err)
	}
}

func credJSON(w http.ResponseWriter) {
	w.Write([]byte(`{"Code":"Success","AccessKeyId":"ASIA","SecretAccessKey":"sk","Token":"st","Expiration":"2030-01-01T00:00:00Z"}`))
}

func TestAWSSourceContainer(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/full" && r.Header.Get("Authorization") != "pod-token" {
			http.Error(w, "no token", 401)
			return
		}
		credJSON(w)
	}))
	defer srv.Close()
	tok := filepath.Join(t.TempDir(), "token")
	os.WriteFile(tok, []byte("pod-token"), 0o600)
	e := defaultAWSEnv()
	e.ecsHost = srv.URL
	for _, env := range []map[string]string{
		{"AWS_CONTAINER_CREDENTIALS_RELATIVE_URI": "/v2/credentials/x"},
		{"AWS_CONTAINER_CREDENTIALS_FULL_URI": srv.URL + "/full", "AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE": tok},
		{"AWS_CONTAINER_CREDENTIALS_FULL_URI": srv.URL + "/full", "AWS_CONTAINER_AUTHORIZATION_TOKEN": "pod-token"},
	} {
		e.getenv = envOf(env)
		c, err := e.source("").fetch(context.Background())
		if err != nil || c.AccessKeyID != "ASIA" || c.SessionToken != "st" {
			t.Errorf("%v: %+v %v", env, c, err)
		}
	}
}

func TestAWSSourceIMDS(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch {
		case r.Method == "PUT" && r.URL.Path == "/latest/api/token":
			w.Write([]byte("imds-token"))
		case r.Header.Get("X-Aws-Ec2-Metadata-Token") != "imds-token":
			http.Error(w, "no token", 401)
		case r.URL.Path == "/latest/meta-data/iam/security-credentials/":
			w.Write([]byte("the-role"))
		case r.URL.Path == "/latest/meta-data/iam/security-credentials/the-role":
			credJSON(w)
		default:
			http.NotFound(w, r)
		}
	}))
	defer srv.Close()
	e := defaultAWSEnv()
	e.getenv = envOf(nil)
	e.imdsURL = srv.URL
	c, err := e.source("").fetch(context.Background())
	if err != nil || c.AccessKeyID != "ASIA" {
		t.Fatalf("%+v %v", c, err)
	}
}

type countingSource struct {
	n   atomic.Int32
	exp time.Time
}

func (s *countingSource) fetch(context.Context) (awsCreds, error) {
	s.n.Add(1)
	return awsCreds{AccessKeyID: "x", Expires: s.exp}, nil
}

func TestAWSCredCacheRefreshesBeforeExpiry(t *testing.T) {
	now := time.Date(2030, 1, 1, 0, 0, 0, 0, time.UTC)
	src := &countingSource{exp: now.Add(time.Hour)}
	c := &awsCredCache{src: src, now: func() time.Time { return now }}
	c.get(context.Background())
	c.get(context.Background())
	if src.n.Load() != 1 {
		t.Fatalf("fetched %d times while fresh", src.n.Load())
	}
	now = now.Add(time.Hour - awsCredSlack + time.Second)
	c.get(context.Background())
	if src.n.Load() != 2 {
		t.Fatalf("fetched %d times near expiry", src.n.Load())
	}
}

func TestGCPServiceAccount(t *testing.T) {
	key, _ := rsa.GenerateKey(rand.Reader, 2048)
	der, _ := x509.MarshalPKCS8PrivateKey(key)
	pemKey := string(pem.EncodeToMemory(&pem.Block{Type: "PRIVATE KEY", Bytes: der}))
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		r.ParseForm()
		if r.Form.Get("grant_type") != "urn:ietf:params:oauth:grant-type:jwt-bearer" {
			http.Error(w, "grant", 400)
			return
		}
		parts := strings.Split(r.Form.Get("assertion"), ".")
		sig, _ := base64.RawURLEncoding.DecodeString(parts[2])
		h := sha256.Sum256([]byte(parts[0] + "." + parts[1]))
		if rsa.VerifyPKCS1v15(&key.PublicKey, crypto.SHA256, h[:], sig) != nil {
			http.Error(w, "signature", 400)
			return
		}
		raw, _ := base64.RawURLEncoding.DecodeString(parts[1])
		var claims map[string]any
		json.Unmarshal(raw, &claims)
		if claims["iss"] != "sa@p.iam.gserviceaccount.com" || claims["scope"] != gcsScope || claims["aud"] != "http://"+r.Host+"/token" {
			http.Error(w, "claims", 400)
			return
		}
		w.Write([]byte(`{"access_token":"ya29.x","expires_in":3599,"token_type":"Bearer"}`))
	}))
	defer srv.Close()
	file, _ := json.Marshal(map[string]string{
		"type": "service_account", "client_email": "sa@p.iam.gserviceaccount.com", "private_key_id": "k1",
		"private_key": pemKey, "token_uri": srv.URL + "/token",
	})
	path := filepath.Join(t.TempDir(), "sa.json")
	os.WriteFile(path, file, 0o600)
	src, err := gcpSourceFromEnv(srv.Client(), envOf(map[string]string{"GOOGLE_APPLICATION_CREDENTIALS": path}))
	if err != nil {
		t.Fatal(err)
	}
	tok, err := src.token(context.Background())
	if err != nil || tok.value != "ya29.x" || time.Until(tok.expires) < 59*time.Minute {
		t.Fatalf("%+v %v", tok, err)
	}
}

func TestGCPServiceAccountRefusesOtherFiles(t *testing.T) {
	path := filepath.Join(t.TempDir(), "user.json")
	os.WriteFile(path, []byte(`{"type":"authorized_user"}`), 0o600)
	if _, err := gcpSourceFromEnv(http.DefaultClient, envOf(map[string]string{"GOOGLE_APPLICATION_CREDENTIALS": path})); err == nil {
		t.Fatal("an authorized_user file was read")
	}
}

func TestGCPMetadataServer(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Metadata-Flavor") != "Google" || r.URL.Path != "/computeMetadata/v1/instance/service-accounts/default/token" {
			http.Error(w, "no", 403)
			return
		}
		w.Write([]byte(`{"access_token":"ya29.m","expires_in":3599}`))
	}))
	defer srv.Close()
	src, _ := gcpSourceFromEnv(srv.Client(), envOf(map[string]string{"GCE_METADATA_HOST": strings.TrimPrefix(srv.URL, "http://")}))
	tok, err := src.token(context.Background())
	if err != nil || tok.value != "ya29.m" {
		t.Fatalf("%+v %v", tok, err)
	}
}

func TestS3ListFollowsContinuation(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if !strings.HasPrefix(r.Header.Get("Authorization"), "AWS4-HMAC-SHA256 Credential=id/") || r.URL.Path != "/bkt" {
			http.Error(w, "unsigned or wrong bucket", 403)
			return
		}
		q := r.URL.Query()
		if q.Get("prefix") != "pre/logs/" || q.Get("start-after") != "pre/logs/a" {
			http.Error(w, "query "+q.Encode(), 400)
			return
		}
		if q.Get("continuation-token") == "" {
			w.Write([]byte(`<ListBucketResult><IsTruncated>true</IsTruncated><NextContinuationToken>t2</NextContinuationToken>
<Contents><Key>pre/logs/b</Key><LastModified>2030-01-01T00:00:00.000Z</LastModified></Contents></ListBucketResult>`))
			return
		}
		w.Write([]byte(`<ListBucketResult><IsTruncated>false</IsTruncated>
<Contents><Key>pre/logs/c</Key><LastModified>2030-01-01T00:00:00.000Z</LastModified></Contents></ListBucketResult>`))
	}))
	defer srv.Close()
	t.Setenv("AWS_ACCESS_KEY_ID", "id")
	t.Setenv("AWS_SECRET_ACCESS_KEY", "sk")
	b, err := NewS3(context.Background(), "bkt", "pre", srv.URL, "")
	if err != nil {
		t.Fatal(err)
	}
	got, err := b.List(context.Background(), "logs/", "logs/a")
	if err != nil || len(got) != 2 || got[0].Key != "logs/b" || got[1].Key != "logs/c" {
		t.Fatalf("%+v %v", got, err)
	}
}

func TestGCSListFollowsPageToken(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		q := r.URL.Query()
		if r.URL.Path != "/storage/v1/b/bkt/o" || q.Get("startOffset") != "pre/logs/a" {
			http.Error(w, "query "+q.Encode(), 400)
			return
		}
		if q.Get("pageToken") == "" {
			w.Write([]byte(`{"items":[{"name":"pre/logs/a","updated":"2030-01-01T00:00:00Z"},{"name":"pre/logs/b","updated":"2030-01-01T00:00:00Z"}],"nextPageToken":"p2"}`))
			return
		}
		w.Write([]byte(`{"items":[{"name":"pre/logs/c","updated":"2030-01-01T00:00:00Z"}]}`))
	}))
	defer srv.Close()
	t.Setenv("STORAGE_EMULATOR_HOST", srv.URL)
	g, _ := NewGCS(context.Background(), "bkt", "pre")
	got, err := g.List(context.Background(), "logs/", "logs/a")
	if err != nil || len(got) != 2 || got[0].Key != "logs/b" || got[1].Key != "logs/c" {
		t.Fatalf("%+v %v", got, err)
	}
}
