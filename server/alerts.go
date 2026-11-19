package main

// The service alerts document and the alert templates document (store
// mode; D-206..D-210): what the agency tells riders, and its standard
// language. The host decides nothing: a write goes through the receive
// path, where cap-alerts validate (or validate_templates) answers every
// refusal (422) or the canonical document, which the alerts log records
// as one entry. A write naming a version that is no longer the head is
// refused (409), and the writer re-reads.
//
// One cap-alerts instance serves every use, emptied after each
// (`withAlerts`).
//
//	POST /service-alerts   body: {"from": "<the head the writer read, or empty>", "alerts": {...}}
//	                       200 {alerts_hash}   422 {refusals}   409 {current, seq}
//	POST /alert-templates  body: {"from": "...", "templates": {...}}
//	                       200 {templates_hash}   422 {refusals}   409 {current, seq}
//
// The manifest pins the heads as service_alerts_hash and
// alert_templates_hash; a page reads each document by hash from /blobs.
// The log is logs/alerts/ in the bucket (GET /logs/alerts). There is no
// feed endpoint (D-124): the alerts document's layout in the bucket is
// what a serving endpoint reads.

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"

	"rustwasm/server/store"
)

const (
	alertsLogName        = "alerts"
	alertsPointer        = "service_alerts"
	templatesPointer     = "alert_templates"
	opLoadServiceAlerts  = 272
	opLoadAlertTemplates = 273
)

// withAlerts runs `use` on the server's one cap-alerts instance, made on
// first use, and then empties it (the kernel's evict), so no document
// outlives the request that sent it.
func (s *server) withAlerts(ctx context.Context, use func(*WasmHost) error) error {
	if s.alertsWasm == "" {
		return fmt.Errorf("no alerts module (start with -alerts-wasm)")
	}
	s.alertsMu.Lock()
	defer s.alertsMu.Unlock()
	if s.alertsHeld == nil {
		host, err := NewWasmHost(ctx, s.alertsWasm)
		if err != nil {
			return fmt.Errorf("alerts module: %w", err)
		}
		s.alertsHeld = host
	}
	err := use(s.alertsHeld)
	var evicted json.RawMessage
	if evictErr := s.alertsHeld.InvokeBytes(ctx, opEvict, []byte(`{"keep":[]}`), &evicted); evictErr != nil && err == nil {
		err = fmt.Errorf("emptying the alerts module: %w", evictErr)
	}
	return err
}

// alertsLog is the server's fold of the alerts log, made on first use.
func (s *server) alertsLog() *logFold {
	s.alertsLogOnce.Do(func() {
		s.alertsFold = &logFold{name: alertsLogName, assemble: s.assembleAlerts}
	})
	return s.alertsFold
}

// assembleAlerts turns the fold's records into the service alerts
// document and the alert templates document, through cap-alerts
// (`log_document`). Caller holds the fold's lock.
func (s *server) assembleAlerts(records map[string]json.RawMessage) error {
	ctx := context.Background()
	s.alertsDoc, s.templatesDoc = logDoc{}, logDoc{}
	for _, k := range []struct {
		record, kind string
		into         *logDoc
	}{
		{"service_alert", "service_alerts", &s.alertsDoc},
		{"alert_template", "alert_templates", &s.templatesDoc},
	} {
		raw, ok := records[k.record]
		if !ok || len(raw) <= 2 || s.alertsWasm == "" {
			continue
		}
		req := map[string]any{"op": "log_document", "kind": k.kind, "records": raw}
		if own := ownRecord(records, k.kind); own != nil {
			req["document"] = own
		}
		var out struct {
			DocumentHash string `json:"document_hash"`
			Document     string `json:"document"`
		}
		if err := s.withAlerts(ctx, func(host *WasmHost) error {
			return host.Invoke(ctx, req, &out)
		}); err != nil {
			return err
		}
		*k.into = logDoc{out.DocumentHash, []byte(out.Document)}
	}
	return nil
}

// alertsLogHead is one of the alerts log's documents in force: its hash
// and bytes, or found false when there is none (or no store).
func (s *server) alertsLogHead(which func() logDoc) (hash string, doc []byte, found bool, err error) {
	if s.store == nil {
		return "", nil, false, nil
	}
	f := s.alertsLog()
	if _, err := s.refreshLog(f); err != nil {
		return "", nil, false, err
	}
	f.mu.Lock()
	defer f.mu.Unlock()
	held := which()
	return held.hash, held.doc, held.hash != "", nil
}

// alertsLogDoc answers the alerts log's document of this hash, if the
// fold holds it: what /blobs serves for the manifest's pins.
func (s *server) alertsLogDoc(hash string) (kind string, doc []byte, ok bool) {
	if s.store == nil || s.alertsFold == nil {
		return "", nil, false
	}
	f := s.alertsFold
	f.mu.Lock()
	defer f.mu.Unlock()
	switch hash {
	case s.alertsDoc.hash:
		return "service_alerts", s.alertsDoc.doc, s.alertsDoc.hash != ""
	case s.templatesDoc.hash:
		return "alert_templates", s.templatesDoc.doc, s.templatesDoc.hash != ""
	}
	return "", nil, false
}

// alertsWrite is one of the two documents a POST writes: its body field,
// its validate op, its load, the log_records argument that names it,
// its held document, and the answer's hash field.
type alertsWrite struct {
	field, validate string
	load            uint32
	loaded          string
	held            func() logDoc
	answer, route   string
}

func (s *server) alertsWrites() map[string]alertsWrite {
	return map[string]alertsWrite{
		"/service-alerts": {field: "alerts", validate: "validate", load: opLoadServiceAlerts, loaded: "alerts_hash",
			held: func() logDoc { return s.alertsDoc }, answer: "alerts_hash", route: "POST /service-alerts"},
		"/alert-templates": {field: "templates", validate: "validate_templates", load: opLoadAlertTemplates, loaded: "templates_hash",
			held: func() logDoc { return s.templatesDoc }, answer: "templates_hash", route: "POST /alert-templates"},
	}
}

// handleAlerts serves POST /service-alerts and POST /alert-templates.
func (s *server) handleAlerts(w http.ResponseWriter, r *http.Request) {
	if s.store == nil {
		http.Error(w, "the service alerts need the store (start with -store <url>)", http.StatusNotFound)
		return
	}
	spec, known := s.alertsWrites()[r.URL.Path]
	if !known {
		http.NotFound(w, r)
		return
	}
	if r.Method != http.MethodPost {
		http.Error(w, "use POST (clients read the documents through the manifest's service_alerts_hash and alert_templates_hash)", http.StatusMethodNotAllowed)
		return
	}
	var req map[string]json.RawMessage
	body, err := io.ReadAll(io.LimitReader(r.Body, 64<<20))
	if err == nil {
		err = json.Unmarshal(body, &req)
	}
	var from string
	if err == nil && len(req["from"]) > 0 {
		err = json.Unmarshal(req["from"], &from)
	}
	if err != nil || len(req[spec.field]) == 0 {
		http.Error(w, fmt.Sprintf(`body: {"from": "<hash or empty>", %q: {...}}`, spec.field), http.StatusBadRequest)
		return
	}
	ctx := r.Context()
	writer := writerOf(r, "anonymous")
	var unknown []string
	f := s.alertsLog()
	_, _, refusal, err := s.writeLog(f, logWrite{writer: writer, op: spec.route, argsHash: store.HashOf(body)}, func() ([]json.RawMessage, *logRefusal, error) {
		if moved := fromMoved(from, spec.held(), f.seq); moved != nil {
			return nil, moved, nil
		}
		var records []json.RawMessage
		var refusal *logRefusal
		err := s.withAlerts(ctx, func(host *WasmHost) error {
			var checked struct {
				Refusals      json.RawMessage `json:"refusals"`
				Document      *string         `json:"document"`
				UnknownFields []string        `json:"unknown_fields"`
			}
			if err := host.Invoke(ctx, map[string]any{"op": spec.validate, spec.field: req[spec.field]}, &checked); err != nil {
				return err
			}
			if checked.Document == nil {
				refusal = &logRefusal{http.StatusUnprocessableEntity, map[string]any{"refusals": checked.Refusals}}
				return nil
			}
			unknown = checked.UnknownFields
			var loaded map[string]any
			if err := host.InvokeBytes(ctx, spec.load, []byte(*checked.Document), &loaded); err != nil {
				return err
			}
			var out logNext
			if err := host.Invoke(ctx, map[string]any{"op": "log_records", spec.loaded: loaded[spec.loaded]}, &out); err != nil {
				return err
			}
			records = out.Next
			return nil
		})
		if err != nil || refusal != nil {
			return nil, refusal, err
		}
		return records, nil, nil
	})
	if err != nil {
		logWriteFailed(w, err, http.StatusBadRequest)
		return
	}
	if refusal != nil {
		writeJSON(w, refusal.status, refusal.body)
		return
	}
	flagUnknown(w, spec.field, writer, unknown)
	f.mu.Lock()
	hash := spec.held().hash
	f.mu.Unlock()
	writeJSON(w, http.StatusOK, map[string]any{spec.answer: hash})
}
