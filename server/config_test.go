package main

// The settings file's laws: a file sets what the command line does not;
// the command line beats every file, a later file beats an earlier; a
// [table] prefixes its keys; a relative path is read from the file's
// directory while a URL or bucket key passes as given; a key that names
// no flag, or a value no flag takes, refuses the boot. And dev.toml, the
// defaults `just serve` reads, names only real flags.

import (
	"flag"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func writeConfig(t *testing.T, dir, name, body string) string {
	t.Helper()
	p := filepath.Join(dir, name)
	if err := os.WriteFile(p, []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	return p
}

func parsed(t *testing.T, args ...string) (*flag.FlagSet, *settings) {
	t.Helper()
	fs := flag.NewFlagSet("server", flag.ContinueOnError)
	f := serverFlags(fs)
	if err := fs.Parse(args); err != nil {
		t.Fatal(err)
	}
	return fs, f
}

func TestConfigSetsWhatTheCommandLineDoesNot(t *testing.T) {
	dir := t.TempDir()
	p := writeConfig(t, dir, "a.toml", `
addr = "127.0.0.1:9999"
store = "mem://"
rt-proxy = true
retention-days = 30
drain = "1s"
publisher = "Test Transit"
`)
	fs, f := parsed(t, "-config", p, "-publisher", "Command Line")
	if err := applyConfig(fs, f.configs); err != nil {
		t.Fatal(err)
	}
	if *f.addr != "127.0.0.1:9999" || *f.storeURL != "mem://" || !*f.rtProxy || *f.retentionDays != 30 || f.drain.String() != "1s" {
		t.Fatalf("file values not applied: %s %s %v %d %v", *f.addr, *f.storeURL, *f.rtProxy, *f.retentionDays, *f.drain)
	}
	if *f.publisher != "Command Line" {
		t.Fatalf("the command line must beat the file, got %q", *f.publisher)
	}
}

func TestALaterFileBeatsAnEarlier(t *testing.T) {
	dir := t.TempDir()
	a := writeConfig(t, dir, "a.toml", "work-orders = \"stub:\"\nattachments = \"mem://\"\n")
	b := writeConfig(t, dir, "b.toml", "work-orders = \"stub:Maintenance\"\n")
	fs, f := parsed(t, "-config", a, "-config", b)
	if err := applyConfig(fs, f.configs); err != nil {
		t.Fatal(err)
	}
	if *f.workOrdersSpec != "stub:Maintenance" || *f.attachmentsURL != "mem://" {
		t.Fatalf("got work-orders %q attachments %q", *f.workOrdersSpec, *f.attachmentsURL)
	}
}

func TestATablePrefixesItsKeys(t *testing.T) {
	dir := t.TempDir()
	p := writeConfig(t, dir, "a.toml", `
[oidc]
issuer = "https://idp.example"
jwks-url = "https://idp.example/jwks"
name-claim = "preferred_username"
`)
	fs, f := parsed(t, "-config", p)
	if err := applyConfig(fs, f.configs); err != nil {
		t.Fatal(err)
	}
	if *f.oidcIssuer != "https://idp.example" || *f.oidcJWKS != "https://idp.example/jwks" || *f.oidcName != "preferred_username" {
		t.Fatalf("got %q %q %q", *f.oidcIssuer, *f.oidcJWKS, *f.oidcName)
	}
}

func TestARelativePathIsReadFromTheFilesDirectory(t *testing.T) {
	dir := t.TempDir()
	p := writeConfig(t, dir, "a.toml", `
feed = "../feeds/gtfs.zip"
streets = "bucket:roads.geojson"
policy = "https://example.org/policy.json"
dist = "/srv/dist"
geocoder = "http://127.0.0.1:4000"
`)
	fs, f := parsed(t, "-config", p)
	if err := applyConfig(fs, f.configs); err != nil {
		t.Fatal(err)
	}
	if want := filepath.Join(dir, "../feeds/gtfs.zip"); *f.feedPath != want {
		t.Fatalf("feed %q, want %q", *f.feedPath, want)
	}
	if *f.streetsPath != "bucket:roads.geojson" || *f.policyFile != "https://example.org/policy.json" || *f.dist != "/srv/dist" || *f.geocoderURL != "http://127.0.0.1:4000" {
		t.Fatalf("a URL, bucket key or absolute path must pass as given: %q %q %q %q", *f.streetsPath, *f.policyFile, *f.dist, *f.geocoderURL)
	}
}

func TestAConfigThatNamesNoFlagRefusesTheBoot(t *testing.T) {
	dir := t.TempDir()
	for name, body := range map[string]string{
		"unknown": "work-order = \"stub:\"\n",
		"nested":  "[oidc]\nisuer = \"https://idp.example\"\n",
		"type":    "retention-days = \"thirty\"\n",
		"array":   "store = [\"mem://\"]\n",
		"itself":  "config = \"other.toml\"\n",
		"missing": "",
		"badtoml": "store = \n",
	} {
		t.Run(name, func(t *testing.T) {
			p := filepath.Join(dir, "absent.toml")
			if name != "missing" {
				p = writeConfig(t, dir, name+".toml", body)
			}
			fs, f := parsed(t, "-config", p)
			err := applyConfig(fs, f.configs)
			if err == nil {
				t.Fatalf("%s: accepted", name)
			}
			if !strings.Contains(err.Error(), filepath.Base(p)) {
				t.Fatalf("%s: the refusal must name the file: %v", name, err)
			}
		})
	}
}

func TestDevConfigNamesRealFlags(t *testing.T) {
	for _, name := range []string{"dev.toml", "local.example.toml"} {
		fs, f := parsed(t, "-config", name)
		if err := applyConfig(fs, f.configs); err != nil {
			t.Fatalf("%s: %v", name, err)
		}
	}
	fs, f := parsed(t, "-config", "dev.toml")
	if err := applyConfig(fs, f.configs); err != nil {
		t.Fatal(err)
	}
	if *f.storeURL != "mem://" || !*f.rtProxy || *f.attachmentsURL == "" || *f.workOrdersSpec == "" {
		t.Fatalf("dev.toml turns on the store, the relay, attachments and work orders: %q %v %q %q", *f.storeURL, *f.rtProxy, *f.attachmentsURL, *f.workOrdersSpec)
	}
	if _, err := os.Stat(*f.feedPath); err != nil {
		t.Fatalf("dev.toml's feed must exist in the tree: %v", err)
	}
}
