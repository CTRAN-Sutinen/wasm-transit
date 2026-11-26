// Package kindroutes serves each kind's `post` route (the kind table's
// `post`; docs/FORMATS.md, "The kind table"). The route's body is the
// arguments of the kind's checking op, and the kind's log receives it as
// one entry (server/logs). The server registers one route for each kind
// that names one, so a grep of the route finds only the table:
// `node tools/caps-run.mjs kind <tag>` prints the path.
package kindroutes

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"

	"rustwasm/server/httpx"
	"rustwasm/server/logs"
	"rustwasm/server/store"
)

type Routes struct {
	logs  *logs.Logs
	store *store.Store
}

// With no store, every route answers 404.
func New(l *logs.Logs, st *store.Store) *Routes { return &Routes{logs: l, store: st} }

// Rule is the route's write rule: its kind's store kind.
func (k *Routes) Rule(r *http.Request) string {
	if route, ok := k.logs.Posts()[r.URL.Path]; ok {
		return route.Kind.Rule()
	}
	return strings.TrimPrefix(r.URL.Path, "/")
}

func (k *Routes) HandleKindPost(w http.ResponseWriter, r *http.Request) {
	if k.store == nil {
		http.Error(w, r.URL.Path+" needs the store (start with -store <url>)", http.StatusNotFound)
		return
	}
	route, ok := k.logs.Posts()[r.URL.Path]
	if !ok {
		http.NotFound(w, r)
		return
	}
	kind, post := route.Kind, route.Kind.Post
	if r.Method != http.MethodPost {
		http.Error(w, fmt.Sprintf("use POST (clients read the document through the manifest's %s_hash)", kind.StoreKind), http.StatusMethodNotAllowed)
		return
	}
	body, err := io.ReadAll(io.LimitReader(r.Body, 64<<20))
	var args map[string]json.RawMessage
	if err == nil {
		err = json.Unmarshal(body, &args)
	}
	if err != nil {
		http.Error(w, "body: a JSON object, the arguments of "+kind.Module+" "+post.Op, http.StatusBadRequest)
		return
	}
	op, _ := json.Marshal(map[string]any{"cap": kind.Module, "op": post.Op, "args": args})
	write := logs.Write{Writer: httpx.Writer(r, "anonymous"), Op: "POST " + post.Path, ArgsHash: store.HashOf(body)}
	_, landed, got, refusal, err := k.logs.Receive(r.Context(), r, route.Log, write, logs.Call{Ops: []json.RawMessage{op}})
	if err != nil {
		logs.WriteFailed(w, err, http.StatusBadRequest)
		return
	}
	if refusal != nil {
		// An op that did not run is the request's 400.
		if got.Failed != nil {
			httpx.AnswerFailed(w, got.Failed.Error, http.StatusBadRequest, kind.StoreKind+": body: ")
			return
		}
		if body, ok := refusal.Body.(map[string]any); ok {
			delete(body, "op")
		}
		logs.WriteRefused(w, refusal)
		return
	}
	httpx.FlagUnknown(w, kind.StoreKind, write.Writer, got.UnknownFields)
	hash := route.Log.Hash(kind.StoreKind)
	if hash == "" {
		hash = got.Heads[kind.StoreKind]
	}
	answer := map[string]any{post.Answer: hash}
	for _, field := range post.Echo {
		if v, ok := args[field]; ok {
			answer[field] = v
		}
	}
	if !landed && len(got.References) > 0 {
		answer["references"] = got.References
	}
	httpx.JSON(w, http.StatusOK, answer)
}
