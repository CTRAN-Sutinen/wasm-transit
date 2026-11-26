package store

import (
	"context"
	"encoding/json"
	"encoding/xml"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strings"
	"sync"
	"time"
)

// The credential sources that DEPLOYMENTS-SERVER.md names, tried in this
// order, and no others:
//
//  1. AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY (and AWS_SESSION_TOKEN)
//  2. EKS web identity: AWS_WEB_IDENTITY_TOKEN_FILE and AWS_ROLE_ARN,
//     exchanged at STS
//  3. ECS container credentials, and EKS Pod Identity:
//     AWS_CONTAINER_CREDENTIALS_RELATIVE_URI or _FULL_URI
//  4. the EC2 instance metadata service (IMDSv2)
//
// The source is chosen once, from the environment, when the client
// opens. Temporary credentials are fetched again shortly before they
// expire.

type awsSource interface {
	fetch(ctx context.Context) (awsCreds, error)
}

// awsCredCache refreshes this long before the credentials expire.
const awsCredSlack = 5 * time.Minute

type awsCredCache struct {
	src  awsSource
	mu   sync.Mutex
	have awsCreds
	ok   bool
	now  func() time.Time
}

func (c *awsCredCache) get(ctx context.Context) (awsCreds, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.ok && (c.have.Expires.IsZero() || c.now().Add(awsCredSlack).Before(c.have.Expires)) {
		return c.have, nil
	}
	cr, err := c.src.fetch(ctx)
	if err != nil {
		return awsCreds{}, fmt.Errorf("s3 credentials: %w", err)
	}
	c.have, c.ok = cr, true
	return cr, nil
}

type awsEnv struct {
	httpc   *http.Client
	getenv  func(string) string
	imdsURL string // the IMDS base; tests point it elsewhere
	ecsHost string // the ECS agent for a relative URI
}

func defaultAWSEnv() awsEnv {
	imds := os.Getenv("AWS_EC2_METADATA_SERVICE_ENDPOINT")
	if imds == "" {
		imds = "http://169.254.169.254"
	}
	return awsEnv{
		httpc:   &http.Client{Timeout: 10 * time.Second},
		getenv:  os.Getenv,
		imdsURL: strings.TrimRight(imds, "/"),
		ecsHost: "http://169.254.170.2",
	}
}

func (e awsEnv) source(region string) awsSource {
	g := e.getenv
	switch {
	case g("AWS_ACCESS_KEY_ID") != "" && g("AWS_SECRET_ACCESS_KEY") != "":
		return staticAWS{awsCreds{
			AccessKeyID:     g("AWS_ACCESS_KEY_ID"),
			SecretAccessKey: g("AWS_SECRET_ACCESS_KEY"),
			SessionToken:    g("AWS_SESSION_TOKEN"),
		}}
	case g("AWS_WEB_IDENTITY_TOKEN_FILE") != "" && g("AWS_ROLE_ARN") != "":
		sts := "https://sts.amazonaws.com"
		if region != "" {
			sts = "https://sts." + region + ".amazonaws.com"
		}
		if v := g("AWS_ENDPOINT_URL_STS"); v != "" {
			sts = strings.TrimRight(v, "/")
		}
		session := g("AWS_ROLE_SESSION_NAME")
		if session == "" {
			session = "wasm-transit"
		}
		return webIdentityAWS{httpc: e.httpc, sts: sts, roleARN: g("AWS_ROLE_ARN"),
			tokenFile: g("AWS_WEB_IDENTITY_TOKEN_FILE"), session: session}
	case g("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI") != "":
		return containerAWS{httpc: e.httpc, url: e.ecsHost + g("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI")}
	case g("AWS_CONTAINER_CREDENTIALS_FULL_URI") != "":
		return containerAWS{httpc: e.httpc, url: g("AWS_CONTAINER_CREDENTIALS_FULL_URI"),
			token: g("AWS_CONTAINER_AUTHORIZATION_TOKEN"), tokenFile: g("AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE")}
	default:
		return imdsAWS{httpc: e.httpc, base: e.imdsURL}
	}
}

type staticAWS struct{ c awsCreds }

func (s staticAWS) fetch(context.Context) (awsCreds, error) { return s.c, nil }

func readBody(resp *http.Response, what string) ([]byte, error) {
	defer resp.Body.Close()
	b, err := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
	if err != nil {
		return nil, err
	}
	if resp.StatusCode/100 != 2 {
		return nil, fmt.Errorf("%s: HTTP %d: %s", what, resp.StatusCode, strings.TrimSpace(string(b)))
	}
	return b, nil
}

type webIdentityAWS struct {
	httpc                           *http.Client
	sts, roleARN, tokenFile, session string
}

func (w webIdentityAWS) fetch(ctx context.Context) (awsCreds, error) {
	tok, err := os.ReadFile(w.tokenFile)
	if err != nil {
		return awsCreds{}, fmt.Errorf("web identity token: %w", err)
	}
	form := url.Values{
		"Action":           {"AssumeRoleWithWebIdentity"},
		"Version":          {"2011-06-15"},
		"RoleArn":          {w.roleARN},
		"RoleSessionName":  {w.session},
		"WebIdentityToken": {strings.TrimSpace(string(tok))},
	}
	req, err := http.NewRequestWithContext(ctx, "POST", w.sts+"/", strings.NewReader(form.Encode()))
	if err != nil {
		return awsCreds{}, err
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	resp, err := w.httpc.Do(req)
	if err != nil {
		return awsCreds{}, err
	}
	body, err := readBody(resp, "sts AssumeRoleWithWebIdentity")
	if err != nil {
		return awsCreds{}, err
	}
	var out struct {
		Result struct {
			Credentials struct {
				AccessKeyID     string    `xml:"AccessKeyId"`
				SecretAccessKey string    `xml:"SecretAccessKey"`
				SessionToken    string    `xml:"SessionToken"`
				Expiration      time.Time `xml:"Expiration"`
			} `xml:"Credentials"`
		} `xml:"AssumeRoleWithWebIdentityResult"`
	}
	if err := xml.Unmarshal(body, &out); err != nil {
		return awsCreds{}, fmt.Errorf("sts answer: %w", err)
	}
	c := out.Result.Credentials
	if c.AccessKeyID == "" {
		return awsCreds{}, errors.New("sts answer holds no credentials")
	}
	return awsCreds{c.AccessKeyID, c.SecretAccessKey, c.SessionToken, c.Expiration}, nil
}

// The JSON that both the ECS agent and IMDS answer.
type awsCredJSON struct {
	Code            string
	AccessKeyID     string `json:"AccessKeyId"`
	SecretAccessKey string
	Token           string
	Expiration      time.Time
}

func (j awsCredJSON) creds(what string) (awsCreds, error) {
	if j.Code != "" && j.Code != "Success" {
		return awsCreds{}, fmt.Errorf("%s: %s", what, j.Code)
	}
	if j.AccessKeyID == "" {
		return awsCreds{}, fmt.Errorf("%s answer holds no credentials", what)
	}
	return awsCreds{j.AccessKeyID, j.SecretAccessKey, j.Token, j.Expiration}, nil
}

type containerAWS struct {
	httpc            *http.Client
	url              string
	token, tokenFile string
}

func (c containerAWS) fetch(ctx context.Context) (awsCreds, error) {
	req, err := http.NewRequestWithContext(ctx, "GET", c.url, nil)
	if err != nil {
		return awsCreds{}, err
	}
	tok := c.token
	if c.tokenFile != "" {
		b, err := os.ReadFile(c.tokenFile)
		if err != nil {
			return awsCreds{}, fmt.Errorf("container authorization token: %w", err)
		}
		tok = strings.TrimSpace(string(b))
	}
	if tok != "" {
		req.Header.Set("Authorization", tok)
	}
	resp, err := c.httpc.Do(req)
	if err != nil {
		return awsCreds{}, err
	}
	body, err := readBody(resp, "container credentials")
	if err != nil {
		return awsCreds{}, err
	}
	var j awsCredJSON
	if err := json.Unmarshal(body, &j); err != nil {
		return awsCreds{}, fmt.Errorf("container credentials: %w", err)
	}
	return j.creds("container credentials")
}

type imdsAWS struct {
	httpc *http.Client
	base  string
}

func (m imdsAWS) fetch(ctx context.Context) (awsCreds, error) {
	do := func(method, path, token string) ([]byte, error) {
		req, err := http.NewRequestWithContext(ctx, method, m.base+path, nil)
		if err != nil {
			return nil, err
		}
		if token == "" {
			req.Header.Set("X-Aws-Ec2-Metadata-Token-Ttl-Seconds", "21600")
		} else {
			req.Header.Set("X-Aws-Ec2-Metadata-Token", token)
		}
		resp, err := m.httpc.Do(req)
		if err != nil {
			return nil, fmt.Errorf("instance metadata (no other credential source is set): %w", err)
		}
		return readBody(resp, "instance metadata "+path)
	}
	tok, err := do("PUT", "/latest/api/token", "")
	if err != nil {
		return awsCreds{}, err
	}
	const roles = "/latest/meta-data/iam/security-credentials/"
	role, err := do("GET", roles, string(tok))
	if err != nil {
		return awsCreds{}, err
	}
	name := strings.TrimSpace(strings.SplitN(string(role), "\n", 2)[0])
	if name == "" {
		return awsCreds{}, errors.New("instance metadata: the machine has no role")
	}
	body, err := do("GET", roles+name, string(tok))
	if err != nil {
		return awsCreds{}, err
	}
	var j awsCredJSON
	if err := json.Unmarshal(body, &j); err != nil {
		return awsCreds{}, fmt.Errorf("instance metadata: %w", err)
	}
	return j.creds("instance metadata")
}
