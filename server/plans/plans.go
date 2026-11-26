// Package plans publishes the agency's service (D-190, D-191). POST /plans
// stores exactly the bytes that cap-network and cap-plans answer, and
// decides nothing itself. The plans log's receive op (cap-plans) writes
// the published plan set (`plans`), which the manifest pins as
// plans_hash, and the frozen versions (`frozen_versions`). The routes are
// in server/openapi.json; `node tools/caps-run.mjs kind plans` prints
// their path.
//
// A frozen network is the head of its plan's plan.<id> pointer, whose log
// keeps every version, so GC never collects a frozen network. The old
// `plans` pointer stays in the bucket, read only, and seeds the log once
// (logs.Seed). A page never writes the plans log by intent, because a
// freeze stores its network first.
package plans

import (
	"context"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net/http"
	"sync"
	"time"

	"rustwasm/server/httpx"
	"rustwasm/server/kinds"
	"rustwasm/server/logs"
	"rustwasm/server/store"
	"rustwasm/server/wasmhost"
)

// Deps is what the host gives the plans. Store and Host are read at each
// call; with either nil, publishing answers 404.
type Deps struct {
	Store func() *store.Store
	Logs  *logs.Logs
	Table func() (*kinds.Table, error)
	// Host speaks as cap-plans.
	Host func() *wasmhost.Host
	// Fresh is an instance of its own for a publish, closed after.
	Fresh func(ctx context.Context) (*wasmhost.Instance, error)
	// Today is the date in the agency's zone, and SetZone gives the host
	// the zone of a published network.
	Today   func() string
	SetZone func(zone string)
	Forward func(kind string, doc json.RawMessage) (json.RawMessage, bool, error)
	// MigratorHash names cap-migrate's module, the writer of a pick
	// brought forward.
	MigratorHash   func() string
	NetworkVersion int
	// FeedZip is the -feed zip, not ok when none is configured.
	FeedZip func() (zip []byte, ok bool, err error)
}

type Plans struct {
	store          func() *store.Store
	logs           *logs.Logs
	table          func() (*kinds.Table, error)
	host           func() *wasmhost.Host
	fresh          func(ctx context.Context) (*wasmhost.Instance, error)
	today          func() string
	setZone        func(zone string)
	forward        func(kind string, doc json.RawMessage) (json.RawMessage, bool, error)
	migratorHash   func() string
	networkVersion int
	feedZip        func() (zip []byte, ok bool, err error)

	feedMu    sync.Mutex
	feedCache map[string][]byte
}

func New(d Deps) *Plans {
	return &Plans{store: d.Store, logs: d.Logs, table: d.Table, host: d.Host, fresh: d.Fresh, today: d.Today, setZone: d.SetZone,
		forward: d.Forward, migratorHash: d.MigratorHash, networkVersion: d.NetworkVersion, feedZip: d.FeedZip}
}

// pickKind is the kind a pick reads (kinds.Pick).
func (p *Plans) pickKind() *kinds.Entry {
	t, err := p.table()
	if err != nil {
		return nil
	}
	return t.PickKind()
}

func (p *Plans) Envelope() store.Envelope {
	return store.Envelope{Kind: "plans", SchemaVersion: 1, ProducedBy: "cap-plans@" + p.host().Hash()}
}

func (p *Plans) HandlePlans(w http.ResponseWriter, r *http.Request) {
	if p.store() == nil || p.host() == nil {
		http.Error(w, "publishing needs the store (start with -store <url>)", http.StatusNotFound)
		return
	}
	if r.Method != http.MethodPost {
		http.Error(w, "use POST (clients read the published set through the manifest's plans_hash)", http.StatusMethodNotAllowed)
		return
	}
	body, err := io.ReadAll(io.LimitReader(r.Body, 128<<20))
	if err != nil {
		http.Error(w, "read body: "+err.Error(), http.StatusBadRequest)
		return
	}
	writer := httpx.Writer(r, "anonymous")
	purpose := r.URL.Query().Get("purpose")
	if purpose == "" {
		purpose = publishPurpose
	}
	out, refused := p.adoptNetwork(r.Context(), body, purpose, "", writer, "POST /plans")
	if refused != nil {
		refused.write(w)
		return
	}
	httpx.FlagUnknown(w, "plans", writer, out.unknownFields)
	httpx.JSON(w, http.StatusOK, map[string]any{
		"plan_id": out.planID, "network_hash": out.networkHash, "plans_hash": out.plansHash, "seq": out.seq, "warnings": out.warnings,
	})
}

type publishedPick struct {
	planID, networkHash, plansHash string
	seq                            int64
	warnings                       json.RawMessage
	unknownFields                  []string
}

type publishRefusal struct {
	code int
	json json.RawMessage // a JSON body, else text
	text string
}

func (p *publishRefusal) write(w http.ResponseWriter) {
	if p.json != nil {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(p.code)
		w.Write(p.json)
		return
	}
	http.Error(w, p.text, p.code)
}

func (p *publishRefusal) Error() string {
	if p.json != nil {
		return fmt.Sprintf("%d %s", p.code, p.json)
	}
	return fmt.Sprintf("%d %s", p.code, p.text)
}

func refusedWith(code int, err error) *publishRefusal {
	return &publishRefusal{code: code, text: err.Error()}
}

const publishPurpose = "published"

func (p *Plans) Publish(ctx context.Context, body []byte, writer string) (publishedPick, *publishRefusal) {
	return p.adoptNetwork(ctx, body, publishPurpose, "", writer, "POST /plans")
}

// adoptNetwork writes one entry of the plans log. With purpose, the
// network is frozen in the same entry. Without, it must be frozen
// already.
func (p *Plans) adoptNetwork(ctx context.Context, body []byte, purpose, from, writer, op string) (publishedPick, *publishRefusal) {
	var out publishedPick
	pick, refused, err := p.writePick(ctx, body, purpose, from, true, writer, op)
	if err != nil {
		return out, refusedWith(http.StatusConflict, err)
	}
	if refused != nil {
		return out, refused
	}
	plansHash, _, _, _ := p.logs.Head("plans")
	return publishedPick{
		planID: pick.planID, networkHash: pick.networkHash, plansHash: plansHash, seq: pick.seq,
		warnings: pick.warnings, unknownFields: pick.unknownFields,
	}, nil
}

// CommitForward runs at startup (docs/FORMATS.md, "Versions"). It
// migrates the networks of the picks in use (cap-plans in_use) and
// republishes each under its own date. A pick that stopped governing,
// or a retired one, stays in its version until it is republished. The
// older blobs stay readable under their hashes.
func (p *Plans) CommitForward() {
	if p.store() == nil || p.host() == nil {
		return
	}
	_, set, found, err := p.logs.Head("plans")
	if err != nil || !found {
		if err != nil {
			log.Printf("commit forward: the plans log: %v", err)
		}
		return
	}
	var used struct {
		Plans []struct {
			ID          string `json:"id"`
			NetworkHash string `json:"network_hash"`
		} `json:"plans"`
	}
	ctx := context.Background()
	if err := p.host().Invoke(ctx, map[string]any{"op": "in_use", "published": json.RawMessage(set), "today": p.today()}, &used); err != nil {
		log.Printf("commit forward: plans: %v", err)
		return
	}
	for _, pick := range used.Plans {
		body, _, err := p.store().GetBlob(pick.NetworkHash)
		if err != nil {
			log.Printf("commit forward: pick %s network %s: %v", pick.ID, pick.NetworkHash, err)
			continue
		}
		doc, migrated, err := p.forward("network", body)
		if err != nil {
			log.Printf("commit forward: pick %s: %v", pick.ID, err)
			continue
		}
		if !migrated {
			continue
		}
		out, refused := p.adoptNetwork(ctx, doc, fmt.Sprintf("a published pick, brought forward to network v%d", p.networkVersion), pick.NetworkHash, "migrate:"+p.migratorHash(), "migrate")
		if refused != nil {
			log.Printf("commit forward: pick %s stays in its version: %v", pick.ID, refused)
			continue
		}
		log.Printf("commit forward: pick %s network %s -> %s (network v%d), plans %s", pick.ID, pick.NetworkHash, out.networkHash, p.networkVersion, out.plansHash)
	}
}

func (p *Plans) HandleRetire(w http.ResponseWriter, r *http.Request) {
	if p.store() == nil || p.host() == nil {
		http.Error(w, "publishing needs the store (start with -store <url>)", http.StatusNotFound)
		return
	}
	if r.Method != http.MethodPost {
		http.Error(w, "use POST", http.StatusMethodNotAllowed)
		return
	}
	var req struct {
		ID string `json:"id"`
	}
	if err := json.NewDecoder(io.LimitReader(r.Body, 1<<20)).Decode(&req); err != nil || req.ID == "" {
		http.Error(w, `body: {"id": "<plan id>"}`, http.StatusBadRequest)
		return
	}
	if _, _, found, err := p.logs.Head("plans"); err != nil || !found {
		if err != nil {
			httpx.Fail(w, err, http.StatusInternalServerError)
		} else {
			http.Error(w, "nothing is published", http.StatusNotFound)
		}
		return
	}
	l := p.logs.Of("plans")
	op, _ := json.Marshal(map[string]any{"cap": "plans", "op": "retire", "args": map[string]any{"id": req.ID, "today": p.today()}})
	seq, refusal, err := p.receivePlans(r.Context(), l, logs.Write{Writer: httpx.Writer(r, "anonymous"), Op: "POST /plans/retire", Note: req.ID}, []json.RawMessage{op})
	if err != nil {
		logs.WriteFailed(w, err, http.StatusInternalServerError)
		return
	}
	if refusal != nil {
		httpx.JSON(w, refusal.Status, refusal.Body)
		return
	}
	plansHash, _, _, _ := p.logs.Head("plans")
	httpx.JSON(w, http.StatusOK, map[string]any{"plans_hash": plansHash, "seq": seq})
}

// HandleFeed serves the pick in force, or the -feed zip before the
// first pick takes effect (D-74). The export is cached by the pick's
// network hash, because a pick's zip is one set of bytes.
func (p *Plans) HandleFeed(w http.ResponseWriter, r *http.Request) {
	date := r.URL.Query().Get("date")
	if date == "" {
		date = p.today()
	}
	if _, err := time.Parse("2006-01-02", date); err != nil {
		http.Error(w, "date: YYYY-MM-DD", http.StatusBadRequest)
		return
	}
	var pick *struct {
		ID          string `json:"id"`
		NetworkHash string `json:"network_hash"`
	}
	var set []byte
	if p.store() != nil && p.host() != nil {
		_, held, found, err := p.logs.Head("plans")
		if err != nil {
			httpx.Fail(w, err, http.StatusInternalServerError)
			return
		}
		if found {
			set = held
			var ans struct {
				Plan *struct {
					ID          string `json:"id"`
					NetworkHash string `json:"network_hash"`
				} `json:"plan"`
			}
			if err := p.host().Invoke(r.Context(), map[string]any{"op": "in_force", "published": json.RawMessage(set), "date": date}, &ans); err != nil {
				httpx.Fail(w, err, http.StatusBadRequest)
				return
			}
			pick = ans.Plan
		}
	}
	var zipBytes []byte
	name := "gtfs.zip"
	if pick != nil {
		var err error
		if zipBytes, err = p.exportedPick(r.Context(), pick.NetworkHash, set); err != nil {
			httpx.Fail(w, err, http.StatusInternalServerError)
			return
		}
		name = "gtfs-" + pick.ID + ".zip"
	} else {
		zip, ok, err := p.feedZip()
		if err != nil {
			httpx.Fail(w, err, http.StatusInternalServerError)
			return
		}
		if !ok {
			http.Error(w, "no feed is in force on "+date, http.StatusNotFound)
			return
		}
		zipBytes = zip
	}
	sum := sha256.Sum256(zipBytes)
	etag := `"sha256:` + hex.EncodeToString(sum[:]) + `"`
	w.Header().Set("ETag", etag)
	if r.Header.Get("If-None-Match") == etag {
		w.WriteHeader(http.StatusNotModified)
		return
	}
	w.Header().Set("Content-Type", "application/zip")
	w.Header().Set("Content-Disposition", `attachment; filename="`+name+`"`)
	w.Write(zipBytes)
}

// exportedPick runs the pick kind's export first when its document
// exists, so a stop whose record changes inside the pick's dates is one
// stop per version. The cache key is the network hash and the
// document's hash, because the bytes never change for those two.
func (p *Plans) exportedPick(ctx context.Context, networkHash string, set []byte) ([]byte, error) {
	e := p.pickKind()
	docHash, doc, haveDoc, err := "", []byte(nil), false, error(nil)
	if e != nil {
		if docHash, doc, haveDoc, err = p.logs.Head(e.StoreKind); err != nil {
			return nil, err
		}
	}
	key := networkHash + "|" + docHash
	p.feedMu.Lock()
	defer p.feedMu.Unlock()
	if zipBytes, ok := p.feedCache[key]; ok {
		return zipBytes, nil
	}
	blob, _, err := p.store().GetBlob(networkHash)
	if err != nil {
		return nil, err
	}
	// A publish runs in an instance of its own, so the networks it reads
	// go with it.
	inst, err := p.fresh(ctx)
	if err != nil {
		return nil, fmt.Errorf("an instance for the export: %w", err)
	}
	defer inst.Close(ctx)
	if haveDoc {
		if blob, err = pickHistory(ctx, inst.As(e.Module), e, blob, doc, set); err != nil {
			return nil, fmt.Errorf("the %s history: %w", e.StoreKind, err)
		}
	}
	host := inst.As("network")
	var loaded struct {
		NetworkHash string `json:"network_hash"`
	}
	if err := host.InvokeBytes(ctx, wasmhost.OpLoadNetwork, blob, &loaded); err != nil {
		return nil, err
	}
	var exported struct {
		FeedHash string `json:"feed_hash"`
		ZipB64   string `json:"zip_b64"`
	}
	if err := host.Invoke(ctx, map[string]any{"op": "export", "network_hash": loaded.NetworkHash}, &exported); err != nil {
		return nil, err
	}
	zipBytes, err := base64.StdEncoding.DecodeString(exported.ZipB64)
	if err != nil {
		return nil, err
	}
	if p.feedCache == nil || len(p.feedCache) >= 4 {
		p.feedCache = map[string][]byte{}
	}
	p.feedCache[key] = zipBytes
	return zipBytes, nil
}

func pickHistory(ctx context.Context, host *wasmhost.Host, e *kinds.Entry, network, doc, set []byte) ([]byte, error) {
	var loadedNet struct {
		NetworkHash string `json:"network_hash"`
	}
	if err := host.InvokeBytes(ctx, wasmhost.OpLoadNetwork, network, &loadedNet); err != nil {
		return nil, fmt.Errorf("the pick's network: %w", err)
	}
	var loaded map[string]any
	if err := host.InvokeBytes(ctx, e.Load.Code, doc, &loaded); err != nil {
		return nil, fmt.Errorf("the %s document: %w", e.StoreKind, err)
	}
	var out struct {
		Network json.RawMessage `json:"network"`
	}
	if err := host.Invoke(ctx, map[string]any{"op": e.Pick.Export, "network_hash": loadedNet.NetworkHash, e.HashField(): wasmhost.LoadedHash(loaded), "published": orNull(set)}, &out); err != nil {
		return nil, err
	}
	return out.Network, nil
}

func withWarnings(warnings json.RawMessage, more []json.RawMessage) json.RawMessage {
	if len(more) == 0 {
		return warnings
	}
	var all []json.RawMessage
	json.Unmarshal(warnings, &all)
	out, _ := json.Marshal(append(all, more...))
	return out
}

func orNull(doc []byte) json.RawMessage {
	if len(doc) == 0 {
		return json.RawMessage("null")
	}
	return json.RawMessage(doc)
}

// warnings holds the network's warnings, then the stops it visits that
// are not in service. unknownFields names the fields that this build
// does not know.
type pickWritten struct {
	planID        string
	networkHash   string
	seq           int64
	warnings      json.RawMessage
	unknownFields []string
}

// writePick writes one `pick` op of cap-plans' receive. It
// stores the network as the head of plan.<id> before the entry, so a
// frozen version is always stored. The routes checked the write rule.
// An error is the server's fault, and a refusal is the writer's.
func (p *Plans) writePick(ctx context.Context, body []byte, purpose, from string, adopt bool, writer, op string) (pickWritten, *publishRefusal, error) {
	var out pickWritten
	l := p.logs.Of("plans")
	if l == nil {
		return out, nil, fmt.Errorf("the kind table names no log for the plans")
	}
	args := map[string]any{"adopt": adopt}
	if purpose != "" {
		args["purpose"] = purpose
	}
	// A migration names the version it came from, and the frozen version
	// pins it (D-232).
	if from != "" {
		args["from_network_hash"] = from
	}
	loads := []logs.Load{{Code: wasmhost.OpLoadNetwork, Doc: body}}
	if e := p.pickKind(); adopt && e != nil {
		// The adoption names the entry of the kind's log that it read (D-191).
		if seq := p.logs.Seq(e.StoreKind); seq > 0 {
			args[e.Pick.Seq] = seq
		}
		_, doc, found, err := p.logs.Head(e.StoreKind)
		if err != nil {
			return out, nil, err
		}
		if found {
			loads = append(loads, logs.Load{Code: e.Load.Code, Doc: doc})
		}
	}
	seq, _, got, refusal, err := p.logs.Receive(ctx, nil, l, logs.Write{Writer: writer, Op: op, ArgsHash: store.HashOf(body)}, logs.Call{
		Loads: loads,
		OnLoad: func(answer map[string]any) {
			if _, network := answer["network_hash"]; !network {
				return
			}
			// today below is a date in the agency's zone.
			if zone, _ := answer["timezone"].(string); zone != "" {
				p.setZone(zone)
			}
			fields, _ := answer["unknown_fields"].([]any)
			for _, f := range fields {
				out.unknownFields = append(out.unknownFields, fmt.Sprint(f))
			}
		},
		OpsAfterLoads: func() []json.RawMessage {
			args["today"] = p.today()
			call, _ := json.Marshal(map[string]any{"cap": "plans", "op": "pick", "args": args})
			return []json.RawMessage{call}
		},
	})
	var refusedLoad *logs.LoadError
	switch {
	case errors.As(err, &refusedLoad):
		return out, refusedWith(http.StatusBadRequest, fmt.Errorf("not a network document: %w", refusedLoad.Err)), nil
	case err != nil:
		return out, nil, err
	case got.Failed != nil:
		return out, nil, &wasmhost.Refused{ModuleError: got.Failed.Error}
	case got.Refused != nil:
		// The 422 body is {issues, refusals} when the network does not
		// publish, and {issues} for a plan issue.
		return out, &publishRefusal{code: http.StatusUnprocessableEntity, json: got.Refused.Refusals}, nil
	case refusal != nil:
		if m, ok := refusal.Body.(map[string]any); ok && refusal.Status == http.StatusBadRequest {
			if e, ok := m["error"].(wasmhost.ModuleError); ok {
				if e.Code != "" {
					body, _ := json.Marshal(map[string]wasmhost.ModuleError{"error": e})
					return out, &publishRefusal{code: refusal.Status, json: body}, nil
				}
				return out, &publishRefusal{code: refusal.Status, text: e.Message}, nil
			}
			return out, &publishRefusal{code: refusal.Status, text: fmt.Sprint(m["error"])}, nil
		}
		body, _ := json.Marshal(refusal.Body)
		return out, &publishRefusal{code: refusal.Status, json: body}, nil
	}
	var answer struct {
		PlanID       string            `json:"plan_id"`
		NetworkHash  string            `json:"network_hash"`
		Warnings     json.RawMessage   `json:"warnings"`
		StopWarnings []json.RawMessage `json:"stop_warnings"`
	}
	if err := json.Unmarshal(got.Answer, &answer); err != nil {
		return out, nil, fmt.Errorf("the pick's answer: %w", err)
	}
	out.planID, out.networkHash, out.seq = answer.PlanID, answer.NetworkHash, seq
	out.warnings = withWarnings(answer.Warnings, answer.StopWarnings)
	return out, nil, nil
}

// receivePlans needs no write check, because the routes made it (the
// policy's `plans`). A refusal is the 422 of the plans' issues.
func (p *Plans) receivePlans(ctx context.Context, l *logs.Log, w logs.Write, ops []json.RawMessage) (int64, *logs.Refusal, error) {
	seq, _, got, refusal, err := p.logs.Receive(ctx, nil, l, w, logs.Call{Ops: ops})
	switch {
	case err != nil:
		return 0, nil, err
	case got.Failed != nil:
		return 0, nil, &wasmhost.Refused{ModuleError: got.Failed.Error}
	case got.Refused != nil:
		return 0, &logs.Refusal{Status: http.StatusUnprocessableEntity, Body: map[string]any{"issues": got.Refused.Refusals}}, nil
	}
	return seq, refusal, nil
}

func (p *Plans) HandleFreeze(w http.ResponseWriter, r *http.Request) {
	if !p.plansWriteReady(w, r) {
		return
	}
	purpose := r.URL.Query().Get("purpose")
	if purpose == "" {
		http.Error(w, "purpose: what the version is (?purpose=proposal for public comment)", http.StatusBadRequest)
		return
	}
	body, err := io.ReadAll(io.LimitReader(r.Body, 128<<20))
	if err != nil {
		http.Error(w, "read body: "+err.Error(), http.StatusBadRequest)
		return
	}
	writer := httpx.Writer(r, "anonymous")
	pick, refused, err := p.writePick(r.Context(), body, purpose, "", false, writer, "POST /plans/freeze")
	if err != nil {
		logs.WriteFailed(w, err, http.StatusInternalServerError)
		return
	}
	if refused != nil {
		refused.write(w)
		return
	}
	httpx.FlagUnknown(w, "plans", writer, pick.unknownFields)
	frozenHash, _, _, _ := p.logs.Head("frozen_versions")
	httpx.JSON(w, http.StatusOK, map[string]any{
		"plan_id": pick.planID, "network_hash": pick.networkHash, "frozen_versions_hash": frozenHash, "seq": pick.seq, "warnings": pick.warnings,
	})
}

func (p *Plans) HandleAdopt(w http.ResponseWriter, r *http.Request) {
	if !p.plansWriteReady(w, r) {
		return
	}
	var req struct {
		NetworkHash string `json:"network_hash"`
	}
	if err := json.NewDecoder(io.LimitReader(r.Body, 1<<16)).Decode(&req); err != nil || req.NetworkHash == "" {
		http.Error(w, `body: {"network_hash": "<a frozen version's network>"}`, http.StatusBadRequest)
		return
	}
	if _, err := store.ParseHash(req.NetworkHash); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	body, _, err := p.store().GetBlob(req.NetworkHash)
	if err == store.ErrNotFound {
		httpx.JSON(w, http.StatusUnprocessableEntity, map[string]any{"issues": []any{map[string]any{"kind": "not_frozen", "network_hash": req.NetworkHash}}})
		return
	}
	if err != nil {
		httpx.Fail(w, err, http.StatusInternalServerError)
		return
	}
	out, refused := p.adoptNetwork(r.Context(), body, "", "", httpx.Writer(r, "anonymous"), "POST /plans/adopt")
	if refused != nil {
		refused.write(w)
		return
	}
	httpx.JSON(w, http.StatusOK, map[string]any{
		"plan_id": out.planID, "network_hash": out.networkHash, "plans_hash": out.plansHash, "seq": out.seq, "warnings": out.warnings,
	})
}

func (p *Plans) plansWriteReady(w http.ResponseWriter, r *http.Request) bool {
	if p.store() == nil || p.host() == nil {
		http.Error(w, "publishing needs the store (start with -store <url>)", http.StatusNotFound)
		return false
	}
	if r.Method != http.MethodPost {
		http.Error(w, "use POST", http.StatusMethodNotAllowed)
		return false
	}
	return true
}
