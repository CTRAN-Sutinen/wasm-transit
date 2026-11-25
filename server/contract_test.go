package main

// Holds the server to server/openapi.json. TestMain replaces
// http.DefaultTransport, so every request and response a test sends is
// checked against the contract. A whole run (no -run or -skip) must also
// reach every operation. `just http-lock` refreshes
// server/openapi.lock.json once the compatibility test passes. The
// server's own outbound requests use `outbound` and are not checked.

import (
	"bytes"
	"encoding/json"
	"flag"
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"io"
	"mime"
	"net/http"
	"net/http/httptest"
	"os"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"sync"
	"testing"

	"github.com/santhosh-tekuri/jsonschema/v6"

	"rustwasm/server/httpcontract"
)

const contractFile = "openapi.json"
const lockFile = "openapi.lock.json"

func TestEveryRouteIsInTheContract(t *testing.T) {
	doc, err := httpcontract.Load(contractFile)
	if err != nil {
		t.Fatal(err)
	}
	inContract := map[string]bool{}
	for p, item := range doc["paths"].(map[string]any) {
		mux, _ := item.(map[string]any)["x-mux"].(string)
		if mux == "" {
			t.Errorf("contract path %s names no x-mux pattern", p)
			continue
		}
		inContract[mux] = true
	}
	served := muxPatterns(t)
	for p := range served {
		if !inContract[p] {
			t.Errorf("the mux serves %s, and the contract does not describe it", p)
		}
	}
	for p := range inContract {
		if !served[p] {
			t.Errorf("the contract describes %s, and the mux does not serve it", p)
		}
	}
}

func muxPatterns(t *testing.T) map[string]bool {
	fset := token.NewFileSet()
	f, err := parser.ParseFile(fset, "main.go", nil, 0)
	if err != nil {
		t.Fatal(err)
	}
	out := map[string]bool{}
	table, err := loadKindTable(defaultKindsPath)
	if err != nil {
		t.Fatal(err)
	}
	for _, k := range table.Kinds {
		if k.Post != nil && k.Receive != "" {
			out[k.Post.Path] = true
		}
		if k.WorkOrder != nil {
			out[k.WorkOrder.Path] = true
		}
	}
	for _, d := range f.Decls {
		fn, ok := d.(*ast.FuncDecl)
		if !ok || fn.Name.Name != "routes" {
			continue
		}
		ast.Inspect(fn, func(n ast.Node) bool {
			call, ok := n.(*ast.CallExpr)
			if !ok || len(call.Args) == 0 {
				return true
			}
			sel, ok := call.Fun.(*ast.SelectorExpr)
			if !ok || (sel.Sel.Name != "HandleFunc" && sel.Sel.Name != "Handle") {
				return true
			}
			if lit, ok := call.Args[0].(*ast.BasicLit); ok {
				p, _ := strconv.Unquote(lit.Value)
				out[p] = true
			}
			return true
		})
	}
	if len(out) == 0 {
		t.Fatal("found no routes in main.go's routes()")
	}
	return out
}

func TestTheContractIsCompatibleWithTheLock(t *testing.T) {
	next, err := httpcontract.Load(contractFile)
	if err != nil {
		t.Fatal(err)
	}
	lock, err := httpcontract.Load(lockFile)
	if err != nil {
		t.Fatalf("%v (the lock is the contract as last released; `just http-lock` writes it)", err)
	}
	for _, b := range httpcontract.Breaks(lock, next) {
		t.Errorf("%s", b)
	}
}

// These operations are otherwise called through a recorder, so the
// contract check would not see them.
func TestTheProbesAndHEADOverHTTP(t *testing.T) {
	dist := t.TempDir()
	if err := os.MkdirAll(dist+"/assets", 0o755); err != nil {
		t.Fatal(err)
	}
	for name, body := range map[string]string{"index.html": "<!doctype html><title>t</title>", "assets/app.js": "export {}"} {
		if err := os.WriteFile(dist+"/"+name, []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	srv := httptest.NewServer((&server{dist: dist}).routes())
	defer srv.Close()
	for _, c := range []struct{ method, path string }{
		{http.MethodGet, "/healthz"}, {http.MethodHead, "/"}, {http.MethodHead, "/assets/app.js"},
	} {
		req, _ := http.NewRequest(c.method, srv.URL+c.path, nil)
		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		resp.Body.Close()
		if resp.StatusCode != http.StatusOK {
			t.Errorf("%s %s: %d", c.method, c.path, resp.StatusCode)
		}
	}
}

type checkedOp struct {
	path, method string
	op           map[string]any
	item         map[string]any
}

type contractCheck struct {
	base     http.RoundTripper
	doc      map[string]any
	compiler *jsonschema.Compiler
	schemas  map[string]*jsonschema.Schema

	mu         sync.Mutex
	violations []string
	exercised  map[string]bool
}

var theContract *contractCheck

func installContractCheck() error {
	raw, err := os.ReadFile(contractFile)
	if err != nil {
		return err
	}
	doc, err := jsonschema.UnmarshalJSON(bytes.NewReader(raw))
	if err != nil {
		return fmt.Errorf("%s: %w", contractFile, err)
	}
	c := jsonschema.NewCompiler()
	c.DefaultDraft(jsonschema.Draft2020)
	if err := c.AddResource("openapi.json", doc); err != nil {
		return err
	}
	theContract = &contractCheck{
		base:      http.DefaultTransport,
		doc:       doc.(map[string]any),
		compiler:  c,
		schemas:   map[string]*jsonschema.Schema{},
		exercised: map[string]bool{},
	}
	http.DefaultTransport = theContract
	return nil
}

func (c *contractCheck) unexercised() []string {
	var out []string
	for p, item := range c.doc["paths"].(map[string]any) {
		for _, m := range httpcontract.Methods {
			if _, ok := item.(map[string]any)[m]; ok {
				where := strings.ToUpper(m) + " " + p
				if !c.exercised[where] {
					out = append(out, where)
				}
			}
		}
	}
	sort.Strings(out)
	return out
}

func reportContract() bool {
	c := theContract
	if c == nil {
		return true
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if flagValue("test.run") == "" && flagValue("test.skip") == "" {
		for _, op := range c.unexercised() {
			c.violations = append(c.violations, op+": no test reaches the operation")
		}
	}
	if len(c.violations) == 0 {
		return true
	}
	sort.Strings(c.violations)
	fmt.Fprintf(os.Stderr, "\nthe HTTP contract (%s) does not admit what the tests saw:\n", contractFile)
	seen := map[string]bool{}
	for _, v := range c.violations {
		if !seen[v] {
			fmt.Fprintf(os.Stderr, "  %s\n", v)
			seen[v] = true
		}
	}
	return false
}

func (c *contractCheck) fail(format string, args ...any) {
	c.mu.Lock()
	c.violations = append(c.violations, fmt.Sprintf(format, args...))
	c.mu.Unlock()
}

func (c *contractCheck) RoundTrip(req *http.Request) (*http.Response, error) {
	var reqBody []byte
	if req.Body != nil && req.Body != http.NoBody {
		b, err := io.ReadAll(req.Body)
		req.Body.Close()
		if err != nil {
			return nil, err
		}
		reqBody = b
		req.Body = io.NopCloser(bytes.NewReader(b))
	}
	resp, err := c.base.RoundTrip(req)
	if err != nil {
		return resp, err
	}
	respBody, err := io.ReadAll(resp.Body)
	resp.Body.Close()
	resp.Body = io.NopCloser(bytes.NewReader(respBody))
	if err != nil {
		return resp, nil
	}
	c.check(req, reqBody, resp, respBody)
	return resp, nil
}

func (c *contractCheck) find(method, path string) (*checkedOp, bool) {
	segs := strings.Split(strings.TrimPrefix(path, "/"), "/")
	best, bestLiterals, pathFound := (*checkedOp)(nil), -1, false
	for p, item := range c.doc["paths"].(map[string]any) {
		tsegs := strings.Split(strings.TrimPrefix(p, "/"), "/")
		segs := fitRest(tsegs, segs)
		if len(tsegs) != len(segs) {
			continue
		}
		literals, ok := 0, true
		for i, ts := range tsegs {
			if !strings.Contains(ts, "{") {
				if ts != segs[i] {
					ok = false
					break
				}
				literals += 2
				continue
			}
			if _, matched := segmentValues(ts, segs[i]); !matched {
				ok = false
				break
			}
			if !(strings.HasPrefix(ts, "{") && strings.HasSuffix(ts, "}") && strings.Count(ts, "{") == 1) {
				literals++ // a template with a literal part beats a bare one.
			}
		}
		if !ok || literals <= bestLiterals {
			continue
		}
		pathFound = true
		im := item.(map[string]any)
		op, _ := im[strings.ToLower(method)].(map[string]any)
		bestLiterals = literals
		if op == nil {
			best = nil
			continue
		}
		best = &checkedOp{path: p, method: strings.ToLower(method), op: op, item: im}
	}
	return best, pathFound
}

func fitRest(tsegs, segs []string) []string {
	n := len(tsegs)
	if n == 0 || !strings.HasSuffix(tsegs[n-1], "...}") || len(segs) < n {
		return segs
	}
	return append(append([]string{}, segs[:n-1]...), strings.Join(segs[n-1:], "/"))
}

func segmentValues(tmpl, seg string) (map[string]string, bool) {
	var names []string
	expr := "^"
	rest := tmpl
	for {
		i := strings.Index(rest, "{")
		if i < 0 {
			expr += regexp.QuoteMeta(rest)
			break
		}
		j := strings.Index(rest[i:], "}")
		if j < 0 {
			return nil, false
		}
		expr += regexp.QuoteMeta(rest[:i]) + "(.+?)"
		names = append(names, strings.TrimSuffix(rest[i+1:i+j], "..."))
		rest = rest[i+j+1:]
	}
	m := regexp.MustCompile(expr + "$").FindStringSubmatch(seg)
	if m == nil {
		return nil, false
	}
	out := map[string]string{}
	for k, n := range names {
		out[n] = m[k+1]
	}
	return out, true
}

func pointerEscape(s string) string {
	return strings.ReplaceAll(strings.ReplaceAll(s, "~", "~0"), "/", "~1")
}

func (c *contractCheck) schema(ptr ...string) (*jsonschema.Schema, error) {
	parts := make([]string, len(ptr))
	for i, p := range ptr {
		parts[i] = pointerEscape(p)
	}
	loc := "openapi.json#/" + strings.Join(parts, "/")
	c.mu.Lock()
	defer c.mu.Unlock()
	if s, ok := c.schemas[loc]; ok {
		return s, nil
	}
	s, err := c.compiler.Compile(loc)
	if err != nil {
		return nil, err
	}
	c.schemas[loc] = s
	return s, nil
}

func mediaType(h string) string {
	mt, _, err := mime.ParseMediaType(h)
	if err != nil {
		return strings.TrimSpace(strings.SplitN(h, ";", 2)[0])
	}
	return mt
}

func content(m map[string]any, mt string) (string, bool) {
	if _, ok := m[mt]; ok {
		return mt, true
	}
	if i := strings.Index(mt, "/"); i > 0 {
		if _, ok := m[mt[:i]+"/*"]; ok {
			return mt[:i] + "/*", true
		}
	}
	if _, ok := m["*/*"]; ok {
		return "*/*", true
	}
	return "", false
}

func (c *contractCheck) check(req *http.Request, reqBody []byte, resp *http.Response, respBody []byte) {
	method, path := req.Method, req.URL.Path
	what := method + " " + path
	op, pathFound := c.find(method, path)
	if op == nil {
		switch {
		case !pathFound && resp.StatusCode == http.StatusNotFound:
		case pathFound && resp.StatusCode == http.StatusMethodNotAllowed:
		case !pathFound:
			c.fail("%s: the contract has no path for it (answered %d)", what, resp.StatusCode)
		default:
			c.fail("%s: the contract has no %s operation there (answered %d)", what, method, resp.StatusCode)
		}
		return
	}
	where := strings.ToUpper(op.method) + " " + op.path
	c.mu.Lock()
	c.exercised[where] = true
	c.mu.Unlock()

	tsegs := strings.Split(strings.TrimPrefix(op.path, "/"), "/")
	segs := fitRest(tsegs, strings.Split(strings.TrimPrefix(path, "/"), "/"))
	pathValues := map[string]string{}
	for i, ts := range tsegs {
		vals, _ := segmentValues(ts, segs[i])
		for k, v := range vals {
			pathValues[k] = v
		}
	}
	type pdef struct {
		in, name string
		m        map[string]any
		idx      int
		fromItem bool
	}
	var defs []pdef
	for i, p := range asList(op.item["parameters"]) {
		pm := p.(map[string]any)
		defs = append(defs, pdef{asStr(pm["in"]), asStr(pm["name"]), pm, i, true})
	}
	for i, p := range asList(op.op["parameters"]) {
		pm := p.(map[string]any)
		defs = append(defs, pdef{asStr(pm["in"]), asStr(pm["name"]), pm, i, false})
	}
	for _, d := range defs {
		var val string
		var present bool
		switch d.in {
		case "query":
			present = req.URL.Query().Has(d.name)
			val = req.URL.Query().Get(d.name)
		case "path":
			val, present = pathValues[d.name]
		case "header":
			val = req.Header.Get(d.name)
			present = val != ""
		default:
			continue
		}
		if !present {
			if req, _ := d.m["required"].(bool); req && resp.StatusCode < 400 {
				c.fail("%s: the required %s parameter %q is missing, and the server answered %d", where, d.in, d.name, resp.StatusCode)
			}
			continue
		}
		if resp.StatusCode >= 400 {
			continue
		}
		base := []string{"paths", op.path}
		if !d.fromItem {
			base = append(base, op.method)
		}
		s, err := c.schema(append(base, "parameters", strconv.Itoa(d.idx), "schema")...)
		if err != nil {
			c.fail("%s parameter %q: %v", where, d.name, err)
			continue
		}
		if err := s.Validate(val); err != nil {
			if err2 := s.Validate(paramValue(val)); err2 != nil {
				c.fail("%s: parameter %q=%q: %v", where, d.name, val, oneLine(err))
			}
		}
	}

	// A request the server refuses (4xx) may be one the contract does not
	// admit.
	if len(reqBody) > 0 && resp.StatusCode < 400 {
		rb, _ := op.op["requestBody"].(map[string]any)
		cm, _ := rb["content"].(map[string]any)
		mt := mediaType(req.Header.Get("Content-Type"))
		key, ok := content(cm, mt)
		switch {
		case !ok:
			c.fail("%s: the request's media type %q is not in the contract (answered %d)", where, mt, resp.StatusCode)
		default:
			c.validateBody(where+" request", []string{"paths", op.path, op.method, "requestBody", "content", key, "schema"}, key, mt, req.Header.Get("Content-Encoding"), reqBody)
		}
	}

	st := strconv.Itoa(resp.StatusCode)
	responses, _ := op.op["responses"].(map[string]any)
	r, ok := responses[st]
	if !ok {
		r, ok = responses[st[:1]+"XX"]
		if ok {
			st = st[:1] + "XX"
		}
	}
	if !ok {
		r, ok = responses["default"]
		st = "default"
	}
	if !ok {
		c.fail("%s: the status %d is not in the contract", where, resp.StatusCode)
		return
	}
	if len(respBody) == 0 || method == http.MethodHead {
		return
	}
	cm, _ := r.(map[string]any)["content"].(map[string]any)
	mt := mediaType(resp.Header.Get("Content-Type"))
	key, ok := content(cm, mt)
	if !ok {
		c.fail("%s %d: the response's media type %q is not in the contract", where, resp.StatusCode, mt)
		return
	}
	c.validateBody(fmt.Sprintf("%s %d", where, resp.StatusCode), []string{"paths", op.path, op.method, "responses", st, "content", key, "schema"}, key, mt, resp.Header.Get("Content-Encoding"), respBody)
}

func (c *contractCheck) validateBody(where string, ptr []string, key, mt, encoding string, body []byte) {
	if encoding != "" || !(mt == "application/json" || strings.HasSuffix(mt, "+json")) {
		return
	}
	cm := c.doc
	var node any = cm
	for _, p := range ptr {
		m, _ := node.(map[string]any)
		node = m[p]
	}
	if node == nil {
		return // no schema: the media type alone is the contract.
	}
	s, err := c.schema(ptr...)
	if err != nil {
		c.fail("%s: %v", where, err)
		return
	}
	v, err := jsonschema.UnmarshalJSON(bytes.NewReader(body))
	if err != nil {
		c.fail("%s: the body is not JSON: %v", where, err)
		return
	}
	if err := s.Validate(v); err != nil {
		c.fail("%s: %s", where, oneLine(err))
	}
}

// paramValue reads a number or boolean as JSON, so a schema of type
// integer or boolean admits it.
func paramValue(s string) any {
	if s == "true" || s == "false" {
		return s == "true"
	}
	if n, err := strconv.ParseInt(s, 10, 64); err == nil {
		return json.Number(strconv.FormatInt(n, 10))
	}
	return s
}

func flagValue(name string) string {
	if f := flag.Lookup(name); f != nil {
		return f.Value.String()
	}
	return ""
}

func oneLine(err error) string {
	return strings.Join(strings.Fields(err.Error()), " ")
}

func asList(v any) []any { l, _ := v.([]any); return l }
func asStr(v any) string { s, _ := v.(string); return s }
