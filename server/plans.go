package main

// Publishing a pick (store mode): POST /plans takes a plan's network
// document — any serialization, gzipped or not — and makes it the
// pick every client resolves (docs/ROADMAP.md 1). The host decides
// nothing:
//
//   - cap-network (a fresh instance, closed after) registers the
//     document, validates it (a network with issues never publishes),
//     and answers its canonical bytes, hash and plan frame;
//   - cap-plans answers the next published plan set — or the reasons
//     the pick would break the chain — as canonical bytes and hash.
//
// The host stores exactly those bytes: the network under the
// plan.<id> pointer (its log keeps every version referenced, so GC
// never collects a frozen network), then the freeze and the adoption
// as one entry of the plans log (planslog.go), whose fold is the set
// the manifest pins as plans_hash. A client reads the set by hash, asks
// cap-plans in_force for the date, and loads that network by hash from
// /blobs.
//
//	POST /plans          body: the network document; ?purpose= what the
//	                     frozen version is (default "published")
//	                     200 {plan_id, network_hash, plans_hash, seq, warnings}
//	                     422 {issues, refusals} (an invalid network, a pick that
//	                         runs no service in its period, or a pick the chain
//	                         refuses — one dated before today, D-76)
//	                     400 (not a network; a plan id that is not a pointer name)
//	GET  /feed?date=     the GTFS zip in force on the date (default today):
//	                     cap-network's export of the pick in force, or the
//	                     -feed zip before the first pick takes effect
//	POST /plans/retire   body: {"id"} — retire a pick that has not taken
//	                     effect (D-75), as one entry: 200 {plans_hash, seq},
//	                     422 {issues}, 404 when nothing is published
//
// "Today" is the host's: the server's local date, or -today on a test
// server. The modules decide what it means.

import (
	"context"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
	"time"

	"rustwasm/server/store"
)

// plansEnvelope is the envelope of every published plan set: produced
// by the hosted cap-plans module.
func (s *server) plansEnvelope() store.Envelope {
	return store.Envelope{Kind: "plans", SchemaVersion: 1, ProducedBy: "cap-plans@" + s.plans.Hash()}
}

// canonicalPick runs a received network through a fresh cap-network:
// register (kernel op 256; gzip or plain), what refuses it as a pick
// (`publishable`: its issues and refusing warnings, answered as the
// 422 body), and the canonical document with its plan frame. The
// instance closes with the call.
func canonicalPick(ctx context.Context, wasmPath string, body []byte) (doc canonicalNetwork, issues json.RawMessage, err error) {
	host, err := NewWasmHost(ctx, wasmPath)
	if err != nil {
		return doc, nil, fmt.Errorf("network module: %w", err)
	}
	defer host.Close(ctx)
	var loaded struct {
		NetworkHash   string   `json:"network_hash"`
		UnknownFields []string `json:"unknown_fields"`
	}
	if err := host.InvokeBytes(ctx, opLoadNetwork, body, &loaded); err != nil {
		return doc, nil, fmt.Errorf("not a network document: %w", err)
	}
	var checked struct {
		Issues   []json.RawMessage `json:"issues"`
		Refusals []json.RawMessage `json:"refusals"`
		Warnings json.RawMessage   `json:"warnings"`
	}
	if err := host.Invoke(ctx, map[string]any{"op": "publishable", "network_hash": loaded.NetworkHash}, &checked); err != nil {
		return doc, nil, err
	}
	if len(checked.Issues)+len(checked.Refusals) > 0 {
		refused, _ := json.Marshal(map[string]any{"issues": checked.Issues, "refusals": checked.Refusals})
		return doc, refused, nil
	}
	if err := host.Invoke(ctx, map[string]any{"op": "document", "network_hash": loaded.NetworkHash}, &doc); err != nil {
		return doc, nil, err
	}
	doc.Warnings = checked.Warnings
	doc.ProducedBy = "cap-network@" + host.Hash()
	doc.UnknownFields = loaded.UnknownFields
	return doc, nil, nil
}

// canonicalNetwork is cap-network's document answer: the bytes, their
// hash, and the plan frame (passed through to cap-plans unread).
type canonicalNetwork struct {
	Document     string          `json:"document"`
	DocumentHash string          `json:"document_hash"`
	Plan         json.RawMessage `json:"plan"`
	Timezone     string          `json:"timezone"`
	Warnings     json.RawMessage `json:"-"`
	ProducedBy   string          `json:"-"`
	// The fields the pick carries that this build does not know (D-167).
	UnknownFields []string `json:"-"`
}

func (s *server) handlePlans(w http.ResponseWriter, r *http.Request) {
	if s.store == nil || s.plans == nil {
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
	writer := writerOf(r, "anonymous")
	purpose := r.URL.Query().Get("purpose")
	if purpose == "" {
		purpose = publishPurpose
	}
	out, refused := s.adoptNetwork(r.Context(), body, purpose, writer, "POST /plans")
	if refused != nil {
		refused.write(w)
		return
	}
	flagUnknown(w, "plans", writer, out.unknownFields)
	writeJSON(w, http.StatusOK, map[string]any{
		"plan_id": out.planID, "network_hash": out.networkHash, "plans_hash": out.plansHash, "seq": out.seq, "warnings": out.warnings,
	})
}

// publishedPick is what one publish committed.
type publishedPick struct {
	planID, networkHash, plansHash string
	seq                            int64
	warnings                       json.RawMessage
	unknownFields                  []string
}

// publishRefusal is why a publish did not commit, as its HTTP answer.
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

// publishPurpose is what a version frozen by POST /plans says it is,
// unless the caller names it (?purpose=).
const publishPurpose = "published"

// publishNetwork is one publish, for POST /plans and for the startup
// commit of a pick's network forward: the network frozen and adopted in
// one entry of the plans log (planslog.go).
func (s *server) publishNetwork(ctx context.Context, body []byte, writer string) (publishedPick, *publishRefusal) {
	return s.adoptNetwork(ctx, body, publishPurpose, writer, "POST /plans")
}

// adoptNetwork puts a network in force: cap-network checks it and
// answers its canonical bytes and plan frame, the pick reads the stops
// document (its warnings, and the stops log entry it read), and
// cap-plans answers the next published set over the frozen versions.
// With purpose, the network is frozen in the same entry; without, it
// must be frozen already. One entry of the plans log under writer.
func (s *server) adoptNetwork(ctx context.Context, body []byte, purpose, writer, op string) (publishedPick, *publishRefusal) {
	var out publishedPick
	pick, issues, err := canonicalPick(ctx, s.networkWasm, body)
	if err != nil {
		return out, refusedWith(http.StatusBadRequest, err)
	}
	if issues != nil {
		return out, &publishRefusal{code: http.StatusUnprocessableEntity, json: issues}
	}
	// The pick names the agency's zone: today below is its date.
	if pick.Timezone != "" {
		s.zoneMu.Lock()
		s.zone = pick.Timezone
		s.zoneMu.Unlock()
	}
	_, current, _, err := s.keptHead("plans")
	if err != nil {
		return out, refusedWith(http.StatusInternalServerError, err)
	}
	// A pick only reads the stops document (D-157): each stop's record
	// on its first date, and the warnings of the stops it visits that
	// are not in service over its period. The adoption names the stops
	// log entry it read (D-191).
	stopsSeq := s.stopsSeqRead()
	stopWarnings, err := s.readPick(ctx, &pick, orNull(current))
	if err != nil {
		return out, refusedWith(http.StatusInternalServerError, err)
	}
	seq, refusal, err := s.writePlan(ctx, planWrite{pick: pick, purpose: purpose, adopt: true, stopsSeq: stopsSeq}, writer, op)
	if err != nil {
		return out, refusedWith(http.StatusConflict, err)
	}
	if refusal != nil {
		if m, ok := refusal.body.(map[string]any); ok && refusal.status == http.StatusBadRequest {
			return out, &publishRefusal{code: refusal.status, text: fmt.Sprint(m["error"])}
		}
		body, _ := json.Marshal(refusal.body)
		return out, &publishRefusal{code: refusal.status, json: body}
	}
	plansHash, _, _, _ := s.keptHead("plans")
	return publishedPick{
		planID: planID(pick), networkHash: pick.DocumentHash, plansHash: plansHash, seq: seq,
		warnings: withWarnings(pick.Warnings, stopWarnings), unknownFields: pick.UnknownFields,
	}, nil
}

// commitPicksForward brings the networks of the picks in use to the
// version the hosted modules read, at startup (docs/FORMATS.md,
// "Versions"). cap-plans answers which picks are in use: the pick in
// force and those still to come. cap-migrate brings an older network
// forward. The pick is then republished under its own date through
// publishNetwork, so cap-plans moves the set's network_hash. The writer
// names the migration module. A pick that stopped governing, or a
// retired one, stays in its version, and comes forward when it is
// republished. The older blobs stay readable under their hashes.
func (s *server) commitPicksForward() {
	if s.store == nil || s.plans == nil {
		return
	}
	_, set, found, err := s.keptHead("plans")
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
	if err := s.plans.Invoke(ctx, map[string]any{"op": "in_use", "published": json.RawMessage(set), "today": s.today()}, &used); err != nil {
		log.Printf("commit forward: plans: %v", err)
		return
	}
	for _, pick := range used.Plans {
		body, _, err := s.store.GetBlob(pick.NetworkHash)
		if err != nil {
			log.Printf("commit forward: pick %s network %s: %v", pick.ID, pick.NetworkHash, err)
			continue
		}
		doc, migrated, err := s.forward("network", body)
		if err != nil {
			log.Printf("commit forward: pick %s: %v", pick.ID, err)
			continue
		}
		if !migrated {
			continue
		}
		m, _ := s.migrator()
		out, refused := s.adoptNetwork(ctx, doc, fmt.Sprintf("a published pick, brought forward to network v%d", networkVersion), "migrate:"+m.Hash(), "migrate")
		if refused != nil {
			log.Printf("commit forward: pick %s stays in its version: %v", pick.ID, refused)
			continue
		}
		log.Printf("commit forward: pick %s network %s -> %s (network v%d), plans %s", pick.ID, pick.NetworkHash, out.networkHash, networkVersion, out.plansHash)
	}
}

// today is the agency's date, "YYYY-MM-DD": the host's clock
// read in the network's zone — the feed's, else the last published
// pick's, as cap-network answered it — never the host's own zone, so a
// server in a UTC container answers the agency's service day. With no
// zone known yet (no feed, nothing published since boot) it is the
// host's local date. -today pins it on a test server.
func (s *server) today() string {
	if s.fixedToday != "" {
		return s.fixedToday
	}
	now := time.Now()
	if s.now != nil {
		now = s.now()
	}
	if loc := s.agencyZone(); loc != nil {
		now = now.In(loc)
	}
	return now.Format("2006-01-02")
}

// agencyZone is the zone today() counts in: the published feed's, else
// the last published pick's; nil when neither is known or the name is
// not a zone the host's tz database has.
func (s *server) agencyZone() *time.Location {
	zone := ""
	if s.feeder != nil {
		if feed, err := s.feeder.Load(); err == nil {
			zone = feed.timezone
		}
	}
	if zone == "" {
		s.zoneMu.Lock()
		zone = s.zone
		s.zoneMu.Unlock()
	}
	if zone == "" {
		return nil
	}
	loc, err := time.LoadLocation(zone)
	if err != nil {
		log.Printf("agency timezone %q: %v (counting today in the host's zone)", zone, err)
		return nil
	}
	return loc
}

// handleRetire retires a pick (D-75): cap-plans answers the next set —
// the pick kept as the tombstone — or why not, and the host commits it.
func (s *server) handleRetire(w http.ResponseWriter, r *http.Request) {
	if s.store == nil || s.plans == nil {
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
	if _, _, found, err := s.keptHead("plans"); err != nil || !found {
		if err != nil {
			fail(w, err, http.StatusInternalServerError)
		} else {
			http.Error(w, "nothing is published", http.StatusNotFound)
		}
		return
	}
	l := s.keptLogOf("plans")
	op, _ := json.Marshal(map[string]any{"cap": "plans", "op": "retire", "args": map[string]any{"id": req.ID, "today": s.today()}})
	seq, refusal, err := s.receivePlans(r.Context(), l, logWrite{writer: writerOf(r, "anonymous"), op: "POST /plans/retire", note: req.ID}, []json.RawMessage{op})
	if err != nil {
		logWriteFailed(w, err, http.StatusInternalServerError)
		return
	}
	if refusal != nil {
		writeJSON(w, refusal.status, refusal.body)
		return
	}
	plansHash, _, _, _ := s.keptHead("plans")
	writeJSON(w, http.StatusOK, map[string]any{"plans_hash": plansHash, "seq": seq})
}

// handleFeed serves the GTFS feed in force on a date (?date=, default
// today): the zip cap-network `export` writes for the published pick
// governing it — cap-plans `in_force` answers which — or, before the
// first pick takes effect, the configured -feed zip itself (D-74).
// 404 when neither runs. The export runs in a fresh cap-network and is
// kept by the pick's network hash: a pick's zip is one set of bytes.
func (s *server) handleFeed(w http.ResponseWriter, r *http.Request) {
	date := r.URL.Query().Get("date")
	if date == "" {
		date = s.today()
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
	if s.store != nil && s.plans != nil {
		_, held, found, err := s.keptHead("plans")
		if err != nil {
			fail(w, err, http.StatusInternalServerError)
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
			if err := s.plans.Invoke(r.Context(), map[string]any{"op": "in_force", "published": json.RawMessage(set), "date": date}, &ans); err != nil {
				fail(w, err, http.StatusBadRequest)
				return
			}
			pick = ans.Plan
		}
	}
	var zipBytes []byte
	name := "gtfs.zip"
	switch {
	case pick != nil:
		var err error
		if zipBytes, err = s.exportedPick(r.Context(), pick.NetworkHash, set); err != nil {
			fail(w, err, http.StatusInternalServerError)
			return
		}
		name = "gtfs-" + pick.ID + ".zip"
	case s.feeder != nil:
		var err error
		if zipBytes, err = s.feeder.Zip(); err != nil {
			fail(w, err, http.StatusInternalServerError)
			return
		}
	default:
		http.Error(w, "no feed is in force on "+date, http.StatusNotFound)
		return
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

// exportedPick is cap-network's export of a published pick's network
// with the stops' history over its period (D-151): with a stops
// document, cap-stops export_pick first — a stop whose record
// changes inside the pick's dates is one stop per version. Kept for the
// process by the pick's network hash and the stops document's (the
// bytes never change for those two).
func (s *server) exportedPick(ctx context.Context, networkHash string, set []byte) ([]byte, error) {
	stopsHash, stops, haveStops, err := "", []byte(nil), false, error(nil)
	if stopsHash, stops, haveStops, err = s.keptHead("stops"); err != nil {
		return nil, err
	}
	key := networkHash + "|" + stopsHash
	s.feedMu.Lock()
	defer s.feedMu.Unlock()
	if zipBytes, ok := s.feedCache[key]; ok {
		return zipBytes, nil
	}
	blob, _, err := s.store.GetBlob(networkHash)
	if err != nil {
		return nil, err
	}
	if haveStops {
		if blob, err = s.stopsVersions(ctx, blob, stops, set); err != nil {
			return nil, fmt.Errorf("the stops' history: %w", err)
		}
	}
	host, err := NewWasmHost(ctx, s.networkWasm)
	if err != nil {
		return nil, fmt.Errorf("network module: %w", err)
	}
	defer host.Close(ctx)
	var loaded struct {
		NetworkHash string `json:"network_hash"`
	}
	if err := host.InvokeBytes(ctx, opLoadNetwork, blob, &loaded); err != nil {
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
	if s.feedCache == nil || len(s.feedCache) >= 4 {
		s.feedCache = map[string][]byte{}
	}
	s.feedCache[key] = zipBytes
	return zipBytes, nil
}

// stopsVersions is the pick's network with the stops' history over its
// period, from its first date until the next pick's: cap-stops
// export_pick, which derives the period from the published set.
func (s *server) stopsVersions(ctx context.Context, network, stops, set []byte) ([]byte, error) {
	var versioned json.RawMessage
	err := s.withModule(ctx, "stops", func(host *WasmHost) error {
		var loadedNet struct {
			NetworkHash string `json:"network_hash"`
		}
		if err := host.InvokeBytes(ctx, opLoadNetwork, network, &loadedNet); err != nil {
			return fmt.Errorf("the pick's network: %w", err)
		}
		var loadedStops struct {
			StopsHash string `json:"stops_hash"`
		}
		if err := host.InvokeBytes(ctx, opLoadStops, stops, &loadedStops); err != nil {
			return fmt.Errorf("the stops document: %w", err)
		}
		var out struct {
			Network json.RawMessage `json:"network"`
		}
		if err := host.Invoke(ctx, map[string]any{"op": "export_pick", "network_hash": loadedNet.NetworkHash, "stops_hash": loadedStops.StopsHash, "published": orNull(set)}, &out); err != nil {
			return err
		}
		versioned = out.Network
		return nil
	})
	return versioned, err
}

func writeJSON(w http.ResponseWriter, code int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(code)
	json.NewEncoder(w).Encode(v)
}

// withWarnings is the pick's warnings (cap-network's, a JSON array)
// followed by `more` (cap-stops' visit warnings).
func withWarnings(warnings json.RawMessage, more []json.RawMessage) json.RawMessage {
	if len(more) == 0 {
		return warnings
	}
	var all []json.RawMessage
	json.Unmarshal(warnings, &all)
	out, _ := json.Marshal(append(all, more...))
	return out
}
