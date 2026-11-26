package main

// The kind table (server/kinds) governs a generic pointer write
// (POST /pointers/<name>). The host reads the table and decides nothing:
// each check is the module's.

import (
	"context"
	"encoding/json"
	"fmt"

	"rustwasm/server/kindroutes"
	"rustwasm/server/kinds"
	"rustwasm/server/logs"
	"rustwasm/server/plans"
	"rustwasm/server/store"
)

func (s *server) kindTable() (*kinds.Table, error) {
	s.kindsOnce.Do(func() {
		if s.kinds != nil {
			return
		}
		path := s.kindsPath
		if path == "" {
			path = kinds.DefaultPath
		}
		s.kinds, s.kindsErr = kinds.Load(path)
	})
	return s.kinds, s.kindsErr
}

// kept builds the logs from the server's fields at first use, so a test
// sets them first.
func (s *server) kept() *logs.Logs {
	s.keptOnce.Do(func() {
		s.keptLogs = logs.New(logs.Deps{Store: s.store, Table: s.kindTable, Long: s.longLived, MayWrite: s.mayWrite, WriteDate: s.writeDate, Forward: s.forward})
	})
	return s.keptLogs
}

// planner builds the plans at first use. It reads the store and cap-plans
// at each call, so a test sets them first.
func (s *server) planner() *plans.Plans {
	s.plannerOnce.Do(func() {
		s.plansRoutes = plans.New(plans.Deps{
			Store: func() *store.Store { return s.store },
			Logs:  s.kept(),
			Table: s.kindTable,
			Host:  func() *WasmHost { return s.plans },
			Fresh: s.fresh,
			Today: s.today,
			SetZone: func(zone string) {
				s.zoneMu.Lock()
				s.zone = zone
				s.zoneMu.Unlock()
			},
			Forward:        s.forward,
			MigratorHash:   func() string { m, _ := s.migrator(); return m.Hash() },
			NetworkVersion: networkVersion,
			FeedZip: func() ([]byte, bool, error) {
				if s.feeder == nil {
					return nil, false, nil
				}
				zip, err := s.feeder.Zip()
				return zip, true, err
			},
		})
	})
	return s.plansRoutes
}

func (s *server) kindRoutes() *kindroutes.Routes {
	return kindroutes.New(s.kept(), s.store)
}

// kindWhere is the first kind the table names that has the field, or nil.
func (s *server) kindWhere(has func(*kinds.Entry) bool) *kinds.Entry {
	t, err := s.kindTable()
	if err != nil {
		return nil
	}
	return t.Where(has)
}

func (s *server) workOrderKind() *kinds.Entry {
	return s.kindWhere(func(e *kinds.Entry) bool { return e.WorkOrder != nil })
}

// With no table, the policy is restricted: a missing table never opens it.
func (s *server) restricted(kind string) bool {
	t, err := s.kindTable()
	if err != nil {
		return kind == "policy"
	}
	e := t.ByStoreKind(kind)
	return e == nil && kind == "policy" || e != nil && e.Read == "restricted"
}

func (s *server) pointerRefusal(name, kind string) (*kinds.Entry, string, error) {
	t, err := s.kindTable()
	if err != nil {
		return nil, "", err
	}
	e, refusal := t.PointerRefusal(name, kind)
	return e, refusal, nil
}

type kindCheck struct {
	answer        json.RawMessage
	document      *string
	documentHash  string
	unknownFields []string
	module        string
}

// A pointer write is rare, so the module instance is fresh and closed
// after.
func (s *server) checkKind(ctx context.Context, e *kinds.Entry, doc json.RawMessage) (kindCheck, error) {
	inst, err := s.fresh(ctx)
	if err != nil {
		return kindCheck{}, fmt.Errorf("an instance for %s: %w", e.Module, err)
	}
	defer inst.Close(ctx)
	host := inst.As(e.Module)
	answer, err := host.InvokeRaw(ctx, map[string]any{"op": e.Validate, e.Tag: doc})
	if err != nil {
		return kindCheck{}, err
	}
	var out struct {
		Document      *string  `json:"document"`
		DocumentHash  string   `json:"document_hash"`
		UnknownFields []string `json:"unknown_fields"`
	}
	if err := json.Unmarshal(answer, &out); err != nil {
		return kindCheck{}, err
	}
	return kindCheck{answer: answer, document: out.Document, documentHash: out.DocumentHash, unknownFields: out.UnknownFields, module: "cap-" + e.Module + "@" + host.Hash()}, nil
}
