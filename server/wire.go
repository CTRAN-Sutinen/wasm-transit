package main

// The wiring: each package, built from the server's fields. A package
// that keeps state is built once, at first use, so a test sets the fields
// first. Each reads the store, sign-in, and the inputs at each call where a
// test or a boot step sets them later.

import (
	"context"
	"net/http"

	"rustwasm/server/attachments"
	"rustwasm/server/authz"
	"rustwasm/server/builds"
	"rustwasm/server/edits"
	"rustwasm/server/imports"
	"rustwasm/server/kindroutes"
	"rustwasm/server/kinds"
	"rustwasm/server/logs"
	"rustwasm/server/manifest"
	"rustwasm/server/plans"
	"rustwasm/server/pointers"
	"rustwasm/server/static"
	"rustwasm/server/store"
	"rustwasm/server/workorders"
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
		s.keptLogs = logs.New(logs.Deps{Store: s.store, Table: s.kindTable, Long: s.longLived, MayWrite: func(r *http.Request, rule string) (bool, string) { return s.gate().MayWrite(r, rule) }, WriteDate: s.writeDate, Forward: s.forward})
	})
	return s.keptLogs
}

// gate checks requests against the policy, as sign-in is set when it is
// called.
func (s *server) gate() *authz.Gate {
	return &authz.Gate{
		Auth:  s.authz,
		Store: func() *store.Store { return s.store },
		LogRule: func(name string) (string, bool) {
			l := s.kept().Get(name)
			if l == nil {
				return "", false
			}
			return l.Rule(), true
		},
	}
}

// published serves the documents derived from the inputs that are
// configured now.
func (s *server) published() *imports.Published {
	return &imports.Published{Feed: s.feeder, Streets: s.streets}
}

// recordFeed records the feed's differences in the log of the kind that
// the table names with feed_events.
func (s *server) recordFeed(ctx context.Context, feed *imports.Feed) error {
	return imports.RecordFeed(ctx, s.kept(), s.kindWhere(func(e *kinds.Entry) bool { return e.FeedEvents != "" }), feed)
}

// edits builds the edit document at first use, from the server's fields.
func (s *server) edits() *edits.Edits {
	s.editsOnce.Do(func() {
		s.editDoc = edits.New(edits.Deps{
			Store:         func() *store.Store { return s.store },
			Logs:          s.kept(),
			Sync:          func() *WasmHost { return s.wasm },
			Forward:       s.forward,
			Path:          s.editsPath,
			RetentionDays: s.retentionDays,
		})
	})
	return s.editDoc
}

// pointers builds the pointer and blob routes at first use. They read the
// store and the published documents at each call.
func (s *server) pointers() *pointers.Pointers {
	s.pointersOnce.Do(func() {
		s.pointerRoutes = pointers.New(pointers.Deps{
			Store:               func() *store.Store { return s.store },
			Table:               s.kindTable,
			Logs:                s.kept(),
			Edits:               s.edits(),
			Fresh:               s.fresh,
			PublishedBlob:       func(hash string) (imports.Doc, bool) { return s.published().Blob(hash) },
			StoredSchemaLink:    s.static().StoredSchemaLink,
			PublishedSchemaLink: s.static().PublishedSchemaLink,
		})
	})
	return s.pointerRoutes
}

// builds keeps the releases as builds at first use, from the server's
// settings.
func (s *server) builds() *builds.Builds {
	s.buildsOnce.Do(func() {
		s.buildSet = builds.New(func() *store.Store { return s.store }, s.dist, s.serverModules, s.buildEvery, s.uiDists)
	})
	return s.buildSet
}

func (s *server) workOrderRoutes() *workorders.Orders {
	return workorders.New(func() *store.Store { return s.store }, s.kept(), func() workorders.System { return s.workOrders }, s.workOrderKind, s.longLived, s.today)
}

func (s *server) static() *static.Static {
	return static.New(func() *store.Store { return s.store }, s.dist, s.builds(), s.feedURL)
}

// manifest serves /manifest at first use. A release that landed in -dist
// is the build the manifest pins.
func (s *server) manifest() *manifest.Publisher {
	s.manifestOnce.Do(func() {
		s.manifests = manifest.New(func() *store.Store { return s.store }, func() (manifest.Manifest, error) {
			s.builds().Sync()
			return s.buildManifest()
		})
	})
	return s.manifests
}

func (s *server) attachmentRoutes() *attachments.Attachments {
	return &attachments.Attachments{Bucket: func() store.Backend { return s.attachments }}
}

// geocode reads the geocoder at each call: nil answers 404.
func (s *server) geocode(w http.ResponseWriter, r *http.Request) { s.geocoder.HandleGeocode(w, r) }

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
			NetworkVersion: imports.NetworkVersion,
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
