package main

// POST /plans stores exactly the bytes that cap-network and cap-plans
// answer, and decides nothing itself. The network goes under the
// plan.<id> pointer, whose log keeps every version referenced, so GC
// never collects a frozen network. The freeze and the adoption are one
// entry of the plans log (planslog.go). Routes: server/openapi.json.

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

func (s *server) plansEnvelope() store.Envelope {
	return store.Envelope{Kind: "plans", SchemaVersion: 1, ProducedBy: "cap-plans@" + s.plans.Hash()}
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
	out, refused := s.adoptNetwork(r.Context(), body, purpose, "", writer, "POST /plans")
	if refused != nil {
		refused.write(w)
		return
	}
	flagUnknown(w, "plans", writer, out.unknownFields)
	writeJSON(w, http.StatusOK, map[string]any{
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

func (s *server) publishNetwork(ctx context.Context, body []byte, writer string) (publishedPick, *publishRefusal) {
	return s.adoptNetwork(ctx, body, publishPurpose, "", writer, "POST /plans")
}

// adoptNetwork writes one entry of the plans log. With purpose, the
// network is frozen in the same entry. Without, it must be frozen
// already.
func (s *server) adoptNetwork(ctx context.Context, body []byte, purpose, from, writer, op string) (publishedPick, *publishRefusal) {
	var out publishedPick
	pick, refused, err := s.writePick(ctx, body, purpose, from, true, writer, op)
	if err != nil {
		return out, refusedWith(http.StatusConflict, err)
	}
	if refused != nil {
		return out, refused
	}
	plansHash, _, _, _ := s.keptHead("plans")
	return publishedPick{
		planID: pick.planID, networkHash: pick.networkHash, plansHash: plansHash, seq: pick.seq,
		warnings: pick.warnings, unknownFields: pick.unknownFields,
	}, nil
}

// commitPicksForward runs at startup (docs/FORMATS.md, "Versions"). It
// migrates the networks of the picks in use (cap-plans in_use) and
// republishes each under its own date. A pick that stopped governing,
// or a retired one, stays in its version until it is republished. The
// older blobs stay readable under their hashes.
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
		out, refused := s.adoptNetwork(ctx, doc, fmt.Sprintf("a published pick, brought forward to network v%d", networkVersion), pick.NetworkHash, "migrate:"+m.Hash(), "migrate")
		if refused != nil {
			log.Printf("commit forward: pick %s stays in its version: %v", pick.ID, refused)
			continue
		}
		log.Printf("commit forward: pick %s network %s -> %s (network v%d), plans %s", pick.ID, pick.NetworkHash, out.networkHash, networkVersion, out.plansHash)
	}
}

// today reads the clock in the agency's zone, so a server in a UTC
// container answers the agency's service day. With no zone known yet,
// it uses the host's local date. -today pins it on a test server.
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

// writeDate is today() for the reference rules of a log write.
// It never loads the feed, because the feed's own import writes the
// stops log while Load holds its lock.
func (s *server) writeDate() string {
	if s.fixedToday != "" {
		return s.fixedToday
	}
	now := time.Now()
	if s.now != nil {
		now = s.now()
	}
	zone := ""
	if s.feeder != nil {
		zone, _ = s.feeder.zone.Load().(string)
	}
	if zone == "" {
		s.zoneMu.Lock()
		zone = s.zone
		s.zoneMu.Unlock()
	}
	if loc, err := time.LoadLocation(zone); zone != "" && err == nil {
		now = now.In(loc)
	}
	return now.Format("2006-01-02")
}

// agencyZone is the published feed's zone, else the last published
// pick's. It is nil when neither is known or the tz database lacks the
// name.
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

// handleFeed serves the pick in force, or the -feed zip before the
// first pick takes effect (D-74). The export is cached by the pick's
// network hash, because a pick's zip is one set of bytes.
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

// exportedPick runs the pick kind's export first when its document
// exists, so a stop whose record changes inside the pick's dates is one
// stop per version. The cache key is the network hash and the
// document's hash, because the bytes never change for those two.
func (s *server) exportedPick(ctx context.Context, networkHash string, set []byte) ([]byte, error) {
	e := s.pickKind()
	docHash, doc, haveDoc, err := "", []byte(nil), false, error(nil)
	if e != nil {
		if docHash, doc, haveDoc, err = s.keptHead(e.StoreKind); err != nil {
			return nil, err
		}
	}
	key := networkHash + "|" + docHash
	s.feedMu.Lock()
	defer s.feedMu.Unlock()
	if zipBytes, ok := s.feedCache[key]; ok {
		return zipBytes, nil
	}
	blob, _, err := s.store.GetBlob(networkHash)
	if err != nil {
		return nil, err
	}
	// A publish runs in an instance of its own, so the networks it reads
	// go with it.
	inst, err := s.fresh(ctx)
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

func pickHistory(ctx context.Context, host *WasmHost, e *kindEntry, network, doc, set []byte) ([]byte, error) {
	var loadedNet struct {
		NetworkHash string `json:"network_hash"`
	}
	if err := host.InvokeBytes(ctx, opLoadNetwork, network, &loadedNet); err != nil {
		return nil, fmt.Errorf("the pick's network: %w", err)
	}
	var loaded map[string]any
	if err := host.InvokeBytes(ctx, e.Load.Code, doc, &loaded); err != nil {
		return nil, fmt.Errorf("the %s document: %w", e.StoreKind, err)
	}
	var out struct {
		Network json.RawMessage `json:"network"`
	}
	if err := host.Invoke(ctx, map[string]any{"op": e.Pick.Export, "network_hash": loadedNet.NetworkHash, e.hashField(): loadedHash(loaded), "published": orNull(set)}, &out); err != nil {
		return nil, err
	}
	return out.Network, nil
}

func writeJSON(w http.ResponseWriter, code int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(code)
	json.NewEncoder(w).Encode(v)
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
