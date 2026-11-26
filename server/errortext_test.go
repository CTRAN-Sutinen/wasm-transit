package main

import (
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"testing"
)

// Pins that no server code branches on an error's text
// (docs/architecture/bus.md, "Answers"). A caller finds an error with
// errors.Is or errors.As, and a module's refusal by its code.
var errorText = regexp.MustCompile(`strings\.(Contains|HasPrefix|HasSuffix|Index|EqualFold)\([^,]*\.Error\(\)`)

func errorTextMatches(source string) []string {
	var hits []string
	for i, line := range strings.Split(source, "\n") {
		if strings.HasPrefix(strings.TrimSpace(line), "//") {
			continue
		}
		if errorText.MatchString(line) {
			hits = append(hits, strconv.Itoa(i+1)+": "+strings.TrimSpace(line))
		}
	}
	return hits
}

func TestTheGateFindsAnErrorTextMatch(t *testing.T) {
	if got := errorTextMatches(`	case strings.Contains(err.Error(), "moved on every attempt"):`); len(got) != 1 {
		t.Fatalf("a text match went unseen: %v", got)
	}
	if got := errorTextMatches(`	case errors.Is(err, logs.ErrMoved):`); len(got) != 0 {
		t.Fatalf("errors.Is is not a text match: %v", got)
	}
}

func TestNoServerCodeMatchesAnErrorsText(t *testing.T) {
	var found []string
	filepath.WalkDir(".", func(path string, d os.DirEntry, err error) error {
		if err != nil || d.IsDir() || !strings.HasSuffix(path, ".go") || strings.HasSuffix(path, "_test.go") {
			return err
		}
		b, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		for _, hit := range errorTextMatches(string(b)) {
			found = append(found, path+": "+hit)
		}
		return nil
	})
	if len(found) > 0 {
		t.Fatalf("server code matches an error's text:\n%s", strings.Join(found, "\n"))
	}
}
