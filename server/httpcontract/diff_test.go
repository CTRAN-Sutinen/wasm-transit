package httpcontract

import (
	"encoding/json"
	"strings"
	"testing"
)

// The compatibility gate (D-81): an older client's request is still
// accepted, an older client still reads every response, and a removal
// passes only after the lock marks it deprecated.

const lock = `{
  "openapi": "3.1.0",
  "paths": {
    "/edits": {
      "post": {
        "parameters": [{"name": "dry", "in": "query", "schema": {"type": "boolean"}}],
        "requestBody": {"required": true, "content": {"application/json": {"schema": {
          "type": "object",
          "properties": {"records": {"type": "array", "items": {"type": "object", "x-document": "edit_record"}}, "note": {"type": "string"}},
          "required": ["records"]
        }}}},
        "responses": {"200": {"content": {"application/json": {"schema": {"$ref": "#/components/schemas/Committed"}}}}}
      }
    },
    "/status": {
      "get": {"responses": {"200": {"content": {"application/json": {"schema": {
        "type": "object",
        "properties": {"state": {"type": "string", "enum": ["up", "down"]}, "old": {"type": "string", "deprecated": true}},
        "required": ["state"]
      }}}}}}
    },
    "/legacy": {"get": {"deprecated": true, "responses": {"204": {}}}}
  },
  "components": {"schemas": {"Committed": {
    "type": "object",
    "properties": {"hash": {"type": "string", "pattern": "^sha256:"}, "rev": {"type": "integer"}},
    "required": ["hash", "rev"]
  }}}
}`

func edit(t *testing.T, change func(d Doc)) (Doc, Doc) {
	t.Helper()
	var old, next Doc
	if err := json.Unmarshal([]byte(lock), &old); err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal([]byte(lock), &next); err != nil {
		t.Fatal(err)
	}
	change(next)
	return old, next
}

func at(d Doc, path ...string) map[string]any {
	m := map[string]any(d)
	for _, p := range path {
		m = obj(m[p])
	}
	return m
}

func editsBody(d Doc) map[string]any {
	return at(d, "paths", "/edits", "post", "requestBody", "content", "application/json", "schema")
}

func statusBody(d Doc) map[string]any {
	return at(d, "paths", "/status", "get", "responses", "200", "content", "application/json", "schema")
}

func mustBreak(t *testing.T, change func(d Doc), want string) {
	t.Helper()
	old, next := edit(t, change)
	got := Breaks(old, next)
	if len(got) == 0 {
		t.Fatalf("the change passed; want a break about %q", want)
	}
	for _, b := range got {
		if strings.Contains(b.String(), want) {
			return
		}
	}
	t.Fatalf("no break mentions %q: %v", want, got)
}

func mustPass(t *testing.T, change func(d Doc)) {
	t.Helper()
	old, next := edit(t, change)
	if got := Breaks(old, next); len(got) != 0 {
		t.Fatalf("an additive change broke: %v", got)
	}
}

func TestTheLockIsCompatibleWithItself(t *testing.T) {
	mustPass(t, func(Doc) {})
}

func TestARequestFieldMadeRequiredBreaks(t *testing.T) {
	mustBreak(t, func(d Doc) {
		editsBody(d)["required"] = []any{"records", "note"}
	}, "now required")
}

func TestANewRequiredRequestFieldBreaks(t *testing.T) {
	// The cap-plans publish case: a host on the older build never sends it.
	mustBreak(t, func(d Doc) {
		b := editsBody(d)
		obj(b["properties"])["today"] = map[string]any{"type": "string"}
		b["required"] = []any{"records", "today"}
	}, "today")
}

func TestANewRequiredParameterBreaks(t *testing.T) {
	mustBreak(t, func(d Doc) {
		op := at(d, "paths", "/edits", "post")
		op["parameters"] = append(arr(op["parameters"]), map[string]any{"name": "author", "in": "query", "required": true, "schema": map[string]any{"type": "string"}})
	}, "new parameter is required")
}

func TestUnknownRequestFieldsRefusedBreaks(t *testing.T) {
	mustBreak(t, func(d Doc) { editsBody(d)["additionalProperties"] = false }, "unknown fields are now refused")
}

func TestARequestTypeNarrowedBreaks(t *testing.T) {
	mustBreak(t, func(d Doc) {
		obj(editsBody(d)["properties"])["note"] = map[string]any{"type": "string", "maxLength": 10}
	}, "maxLength")
}

func TestAResponseFieldRemovedBreaks(t *testing.T) {
	mustBreak(t, func(d Doc) {
		c := at(d, "components", "schemas", "Committed")
		delete(obj(c["properties"]), "rev")
		c["required"] = []any{"hash"}
	}, "rev")
}

func TestAResponseFieldMadeOptionalBreaks(t *testing.T) {
	mustBreak(t, func(d Doc) {
		at(d, "components", "schemas", "Committed")["required"] = []any{"hash"}
	}, "no longer always present")
}

func TestAResponseEnumWidenedBreaks(t *testing.T) {
	mustBreak(t, func(d Doc) {
		obj(statusBody(d)["properties"])["state"] = map[string]any{"type": "string", "enum": []any{"up", "down", "draining"}}
	}, "draining")
}

func TestAResponseTypeChangedBreaks(t *testing.T) {
	mustBreak(t, func(d Doc) {
		obj(at(d, "components", "schemas", "Committed")["properties"])["rev"] = map[string]any{"type": "string"}
	}, "type string")
}

func TestADocumentKindChangedBreaks(t *testing.T) {
	mustBreak(t, func(d Doc) {
		obj(obj(editsBody(d)["properties"])["records"])["items"] = map[string]any{"type": "object", "x-document": "trip_cancel"}
	}, "document kind")
}

func TestARouteRemovedBreaks(t *testing.T) {
	mustBreak(t, func(d Doc) { delete(at(d, "paths"), "/edits") }, "path is removed")
}

func TestAMethodRemovedBreaks(t *testing.T) {
	mustBreak(t, func(d Doc) {
		at(d, "paths", "/edits")["put"] = at(d, "paths", "/edits", "post")
		delete(at(d, "paths", "/edits"), "post")
	}, "method is removed")
}

func TestAMediaTypeNoLongerAcceptedBreaks(t *testing.T) {
	mustBreak(t, func(d Doc) {
		c := at(d, "paths", "/edits", "post", "requestBody", "content")
		c["application/gzip"] = c["application/json"]
		delete(c, "application/json")
	}, "no longer accepted")
}

func TestAKeywordOutsideTheSubsetIsRefused(t *testing.T) {
	mustBreak(t, func(d Doc) {
		statusBody(d)["allOf"] = []any{map[string]any{"type": "object"}}
	}, "allOf")
}

func TestAnAdditiveChangePasses(t *testing.T) {
	mustPass(t, func(d Doc) {
		obj(editsBody(d)["properties"])["author"] = map[string]any{"type": "string"}
		op := at(d, "paths", "/edits", "post")
		op["parameters"] = append(arr(op["parameters"]), map[string]any{"name": "since", "in": "query", "schema": map[string]any{"type": "integer"}})
		obj(at(d, "components", "schemas", "Committed")["properties"])["at"] = map[string]any{"type": "string"}
		at(d, "paths", "/edits", "post", "responses")["409"] = map[string]any{"content": map[string]any{"text/plain": map[string]any{"schema": map[string]any{"type": "string"}}}}
		at(d, "paths")["/edits/batch"] = map[string]any{"post": map[string]any{"responses": map[string]any{"200": map[string]any{}}}}
		obj(statusBody(d)["properties"])["state"] = map[string]any{"type": "string", "enum": []any{"up"}}
	})
}

func TestARemovalTheLockMarksDeprecatedPasses(t *testing.T) {
	mustPass(t, func(d Doc) {
		delete(at(d, "paths"), "/legacy")
		delete(obj(statusBody(d)["properties"]), "old")
	})
}

func TestTheExpandStepPasses(t *testing.T) {
	mustPass(t, func(d Doc) {
		obj(obj(editsBody(d)["properties"])["note"])["deprecated"] = true
		at(d, "paths", "/status", "get")["deprecated"] = true
	})
}
