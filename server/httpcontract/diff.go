// Package httpcontract compares server/openapi.json with the lock,
// server/openapi.lock.json, the contract as last released.
//
// The new contract must accept every request that the lock accepts, and
// may answer only what the lock answers. A removal is a break unless the
// lock already marks it `deprecated`, so the expand step is released
// first. The schemas use a subset of JSON Schema, and Breaks refuses a
// keyword outside it.
package httpcontract

import (
	"encoding/json"
	"fmt"
	"os"
	"reflect"
	"sort"
	"strings"
)

type Doc = map[string]any

func Load(path string) (Doc, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var d Doc
	if err := json.Unmarshal(b, &d); err != nil {
		return nil, fmt.Errorf("%s: %w", path, err)
	}
	return d, nil
}

type Break struct {
	Where string
	Why   string
}

func (b Break) String() string { return b.Where + ": " + b.Why }

var Methods = []string{"get", "head", "post", "put", "patch", "delete", "options"}

func Breaks(lock, next Doc) []Break {
	c := &comparer{old: lock, new: next}
	c.unknownKeywords(lock, "lock")
	c.unknownKeywords(next, "contract")
	oldPaths, newPaths := obj(lock["paths"]), obj(next["paths"])
	for _, p := range sortedKeys(oldPaths) {
		oldItem := obj(oldPaths[p])
		newItem, ok := newPaths[p]
		if !ok {
			if !allDeprecated(oldItem) {
				c.add(p, "the path is removed, and the lock does not mark it deprecated")
			}
			continue
		}
		for _, m := range Methods {
			oldOp, ok := oldItem[m]
			if !ok {
				continue
			}
			where := strings.ToUpper(m) + " " + p
			newOp, ok := obj(newItem)[m]
			if !ok {
				if !isTrue(obj(oldOp)["deprecated"]) {
					c.add(where, "the method is removed, and the lock does not mark it deprecated")
				}
				continue
			}
			c.operation(where, oldItem, obj(oldOp), obj(newItem), obj(newOp))
		}
	}
	return c.breaks
}

type comparer struct {
	old, new Doc
	breaks   []Break
	seen     map[string]bool
}

func (c *comparer) add(where, why string) { c.breaks = append(c.breaks, Break{where, why}) }

var allowed = map[string]bool{
	"type": true, "properties": true, "required": true, "additionalProperties": true,
	"items": true, "enum": true, "const": true, "oneOf": true, "$ref": true,
	"description": true, "deprecated": true, "title": true, "examples": true,
	"x-document": true,
	"format":     true, "pattern": true, "minimum": true, "maximum": true,
	"minLength": true, "maxLength": true, "minItems": true, "maxItems": true,
}

// A new or changed constraint is compared by equality.
var constraints = []string{"format", "pattern", "minimum", "maximum", "minLength", "maxLength", "minItems", "maxItems"}

func (c *comparer) unknownKeywords(d Doc, name string) {
	var walk func(s any, where string)
	walk = func(s any, where string) {
		m, ok := s.(map[string]any)
		if !ok {
			return
		}
		for _, k := range sortedKeys(m) {
			if !allowed[k] {
				c.add(name+" "+where, fmt.Sprintf("the schema keyword %q is outside the contract's subset", k))
			}
		}
		if r, ok := m["$ref"].(string); ok && !strings.HasPrefix(r, "#/components/schemas/") {
			c.add(name+" "+where, "a $ref must name #/components/schemas/<name>")
		}
		if ap, ok := m["additionalProperties"]; ok && ap != false {
			c.add(name+" "+where, "additionalProperties may only be false")
		}
		for _, k := range sortedKeys(obj(m["properties"])) {
			walk(obj(m["properties"])[k], where+"."+k)
		}
		if it, ok := m["items"]; ok {
			walk(it, where+"[]")
		}
		for i, b := range arr(m["oneOf"]) {
			walk(b, fmt.Sprintf("%s.oneOf[%d]", where, i))
		}
	}
	for _, k := range sortedKeys(obj(obj(d["components"])["schemas"])) {
		walk(obj(obj(d["components"])["schemas"])[k], "#/components/schemas/"+k)
	}
	for _, p := range sortedKeys(obj(d["paths"])) {
		item := obj(obj(d["paths"])[p])
		for _, prm := range arr(item["parameters"]) {
			walk(obj(prm)["schema"], p+" parameter "+str(obj(prm)["name"]))
		}
		for _, m := range Methods {
			op, ok := item[m]
			if !ok {
				continue
			}
			where := strings.ToUpper(m) + " " + p
			for _, prm := range arr(obj(op)["parameters"]) {
				walk(obj(prm)["schema"], where+" parameter "+str(obj(prm)["name"]))
			}
			for _, ct := range sortedKeys(obj(obj(obj(op)["requestBody"])["content"])) {
				walk(obj(obj(obj(obj(op)["requestBody"])["content"])[ct])["schema"], where+" request "+ct)
			}
			for _, st := range sortedKeys(obj(obj(op)["responses"])) {
				content := obj(obj(obj(obj(op)["responses"])[st])["content"])
				for _, ct := range sortedKeys(content) {
					walk(obj(content[ct])["schema"], where+" "+st+" "+ct)
				}
			}
		}
	}
}

type paramKey struct{ in, name string }

func params(item, op map[string]any) map[paramKey]map[string]any {
	out := map[paramKey]map[string]any{}
	for _, list := range []any{item["parameters"], op["parameters"]} {
		for _, p := range arr(list) {
			pm := obj(p)
			out[paramKey{str(pm["in"]), str(pm["name"])}] = pm
		}
	}
	return out
}

func (c *comparer) operation(where string, oldItem, oldOp, newItem, newOp map[string]any) {
	oldPs, newPs := params(oldItem, oldOp), params(newItem, newOp)
	keys := make([]paramKey, 0, len(oldPs)+len(newPs))
	for k := range oldPs {
		keys = append(keys, k)
	}
	for k := range newPs {
		if _, ok := oldPs[k]; !ok {
			keys = append(keys, k)
		}
	}
	sort.Slice(keys, func(i, j int) bool { return keys[i].in+keys[i].name < keys[j].in+keys[j].name })
	for _, k := range keys {
		pw := fmt.Sprintf("%s %s parameter %q", where, k.in, k.name)
		o, inOld := oldPs[k]
		n, inNew := newPs[k]
		switch {
		case inOld && !inNew:
			if !isTrue(o["deprecated"]) {
				c.add(pw, "the parameter is removed, and the lock does not mark it deprecated")
			}
		case !inOld && inNew:
			if isTrue(n["required"]) {
				c.add(pw, "a new parameter is required, so an older client's request fails")
			}
		default:
			if isTrue(n["required"]) && !isTrue(o["required"]) {
				c.add(pw, "the parameter is now required, so an older client's request fails")
			}
			c.subset(pw, o["schema"], n["schema"], true)
		}
	}

	// The request body: what an older client sends.
	oldBody, newBody := obj(oldOp["requestBody"]), obj(newOp["requestBody"])
	if isTrue(newBody["required"]) && !isTrue(oldBody["required"]) {
		c.add(where+" request", "the body is now required, so an older client's request fails")
	}
	oldContent, newContent := obj(oldBody["content"]), obj(newBody["content"])
	for _, ct := range sortedKeys(oldContent) {
		n, ok := newContent[ct]
		if !ok {
			c.add(where+" request "+ct, "the media type is no longer accepted")
			continue
		}
		c.subset(where+" request "+ct, obj(oldContent[ct])["schema"], obj(n)["schema"], true)
	}

	// The responses: what an older client reads. A new status is
	// allowed; a client takes an unknown status as its class.
	oldResp, newResp := obj(oldOp["responses"]), obj(newOp["responses"])
	for _, st := range sortedKeys(newResp) {
		o, ok := oldResp[st]
		if !ok {
			continue
		}
		oc, nc := obj(obj(o)["content"]), obj(obj(newResp[st])["content"])
		for _, ct := range sortedKeys(nc) {
			was, ok := oc[ct]
			if !ok {
				c.add(where+" "+st+" "+ct, "the response has a media type the lock does not have")
				continue
			}
			c.subset(where+" "+st+" "+ct, obj(nc[ct])["schema"], obj(was)["schema"], false)
		}
	}
}

// subset reports where schema a admits a value that schema b does not.
// For a request, a is the lock's and b the new one's. For a response, a
// is the new one's and b the lock's. oldIsA names the lock's side, for
// the removal rule.
func (c *comparer) subset(where string, a, b any, oldIsA bool) {
	aDoc, bDoc := c.new, c.old
	if oldIsA {
		aDoc, bDoc = c.old, c.new
	}
	am, aref := resolve(aDoc, a)
	bm, bref := resolve(bDoc, b)
	if aref != "" || bref != "" {
		key := fmt.Sprintf("%v|%s|%s", oldIsA, aref, bref)
		if c.seen == nil {
			c.seen = map[string]bool{}
		}
		if c.seen[key] {
			return
		}
		c.seen[key] = true
	}
	if len(bm) == 0 || onlyNotes(bm) {
		return // b admits anything.
	}
	if len(am) == 0 || onlyNotes(am) {
		c.add(where, "the schema was open and is now narrowed")
		return
	}

	if bo, ok := bm["oneOf"]; ok {
		if ao, ok := am["oneOf"]; ok {
			for i, ab := range arr(ao) {
				if !c.fitsSome(ab, arr(bo), oldIsA) {
					c.add(fmt.Sprintf("%s.oneOf[%d]", where, i), "the branch fits no branch on the other side")
				}
			}
			return
		}
		if !c.fitsSome(am, arr(bo), oldIsA) {
			c.add(where, "the schema fits no branch on the other side")
		}
		return
	}
	if ao, ok := am["oneOf"]; ok {
		for i, ab := range arr(ao) {
			c.subset(fmt.Sprintf("%s.oneOf[%d]", where, i), ab, bm, oldIsA)
		}
		return
	}

	if ad, bd := am["x-document"], bm["x-document"]; ad != bd && (ad != nil || bd != nil) {
		c.add(where, fmt.Sprintf("the document kind changes (%v, %v)", ad, bd))
	}

	at, bt := types(am), types(bm)
	if bt != nil {
		if at == nil {
			c.add(where, "the type was open and is now narrowed")
		} else {
			for t := range at {
				if !bt[t] && !(t == "integer" && bt["number"]) {
					c.add(where, fmt.Sprintf("the type %s is not admitted on the other side", t))
				}
			}
		}
	}

	if be, ok := enumOf(bm); ok {
		ae, ok := enumOf(am)
		if !ok {
			c.add(where, "the values are now limited to an enum")
		}
		for _, v := range ae {
			if !containsJSON(be, v) {
				c.add(where, fmt.Sprintf("the value %v is not admitted on the other side", v))
			}
		}
	}

	for _, k := range constraints {
		bv, ok := bm[k]
		if !ok {
			continue
		}
		if av, ok := am[k]; !ok || !reflect.DeepEqual(av, bv) {
			c.add(where, fmt.Sprintf("the %s constraint changes", k))
		}
	}

	aProps, bProps := obj(am["properties"]), obj(bm["properties"])
	oldProps, newProps := bProps, aProps
	if oldIsA {
		oldProps, newProps = aProps, bProps
	}
	for _, p := range sortedKeys(oldProps) {
		if _, ok := newProps[p]; !ok {
			om, _ := resolve(c.old, oldProps[p])
			if !isTrue(om["deprecated"]) {
				c.add(where+"."+p, "the field is removed, and the lock does not mark it deprecated")
			}
		}
	}
	for _, p := range sortedKeys(aProps) {
		if bp, ok := bProps[p]; ok {
			c.subset(where+"."+p, aProps[p], bp, oldIsA)
		} else if bm["additionalProperties"] == false {
			c.add(where+"."+p, "the field is refused on the other side")
		}
	}
	aReq := strSet(am["required"])
	for _, r := range sortedKeys(strSetAny(bm["required"])) {
		if !aReq[r] {
			if oldIsA {
				c.add(where+"."+r, "the field is now required, so an older client's request fails")
			} else {
				c.add(where+"."+r, "the field is no longer always present, so an older client can miss it")
			}
		}
	}
	if bm["additionalProperties"] == false && am["additionalProperties"] != false {
		c.add(where, "unknown fields are now refused")
	}

	if bi, ok := bm["items"]; ok {
		c.subset(where+"[]", am["items"], bi, oldIsA)
	}
}

func (c *comparer) fitsSome(a any, branches []any, oldIsA bool) bool {
	for _, b := range branches {
		trial := &comparer{old: c.old, new: c.new, seen: c.seen}
		trial.subset("", a, b, oldIsA)
		if len(trial.breaks) == 0 {
			return true
		}
	}
	return false
}

// resolve answers "" as the ref when it followed none.
func resolve(d Doc, s any) (map[string]any, string) {
	m := obj(s)
	ref := ""
	for i := 0; i < 32; i++ {
		r, ok := m["$ref"].(string)
		if !ok {
			return m, ref
		}
		ref = r
		m = obj(obj(obj(d["components"])["schemas"])[strings.TrimPrefix(r, "#/components/schemas/")])
	}
	return m, ref
}

func onlyNotes(m map[string]any) bool {
	for k := range m {
		switch k {
		case "description", "title", "examples", "deprecated":
		default:
			return false
		}
	}
	return true
}

func types(m map[string]any) map[string]bool {
	switch t := m["type"].(type) {
	case string:
		return map[string]bool{t: true}
	case []any:
		out := map[string]bool{}
		for _, v := range t {
			out[str(v)] = true
		}
		return out
	}
	return nil
}

func enumOf(m map[string]any) ([]any, bool) {
	if v, ok := m["const"]; ok {
		return []any{v}, true
	}
	if v, ok := m["enum"]; ok {
		return arr(v), true
	}
	return nil, false
}

func containsJSON(list []any, v any) bool {
	for _, x := range list {
		if reflect.DeepEqual(x, v) {
			return true
		}
	}
	return false
}

func allDeprecated(item map[string]any) bool {
	found := false
	for _, m := range Methods {
		op, ok := item[m]
		if !ok {
			continue
		}
		found = true
		if !isTrue(obj(op)["deprecated"]) {
			return false
		}
	}
	return found
}

func obj(v any) map[string]any {
	m, _ := v.(map[string]any)
	return m
}

func arr(v any) []any {
	a, _ := v.([]any)
	return a
}

func str(v any) string {
	s, _ := v.(string)
	return s
}

func isTrue(v any) bool { b, _ := v.(bool); return b }

func strSet(v any) map[string]bool {
	out := map[string]bool{}
	for _, x := range arr(v) {
		out[str(x)] = true
	}
	return out
}

func strSetAny(v any) map[string]any {
	out := map[string]any{}
	for _, x := range arr(v) {
		out[str(x)] = true
	}
	return out
}

func sortedKeys[V any](m map[string]V) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	sort.Strings(out)
	return out
}
