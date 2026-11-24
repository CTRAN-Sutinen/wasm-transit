//! The kinds: one line for each kind of document, in one list
//! ([`KINDS`]). A kind exists only if it has a line here. Nothing
//! discovers kinds: no link-time registration, and no scan of the
//! crates.
//!
//! A line states the kind's name and tag, its library type and the
//! crate that keeps its schema lock, its kernel load when a module reads
//! it from the bus, and how the store keeps it when the store does: how
//! it is written, its routes and pointers, its log and the log's record
//! kinds, the capability that checks a write, the merge rule, and who
//! reads it. The fields that name other records are declared in the
//! kind's schema (`x-references`, D-232), not here.
//!
//! The kind table (`crates/cap-abi/kinds.json`, D-202) is generated
//! from this list and the locked schemas. It is checked in, so a new kind
//! shows in a review's diff, and gen-manifest stops while the table
//! differs from the list (`just schema-lock` writes it again). It is also
//! written beside the server's modules as `web/pkg/kinds.json`.
//!
//! A line is data. No module reads the list: a kind costs a module nothing
//! until a capability names it (`capability! { documents: [...] }`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One kind of document.
#[derive(Debug, Clone, Copy)]
pub struct Declaration {
    /// The kind's name: the registry's documents index, the manifest's
    /// `speaks`, and a content-hash reference's target name it.
    pub name: &'static str,
    /// The tag the document carries inside it, which names its schema.
    /// Empty for a kind with no schema of ours (a GTFS zip, a file of a
    /// build).
    pub tag: &'static str,
    /// The library type that reads and writes it (empty for a kind the
    /// Go host owns, or that has no type).
    pub library: &'static str,
    /// The crate that keeps its schema lock (`crates/<owner>/locks/`),
    /// `server` for the Go host's (`server/schemas/`), or empty.
    pub owner: &'static str,
    /// Its kernel load, when a module reads it from the bus.
    pub load: Option<KernelLoad>,
    /// How the store keeps it, when the store does.
    pub store: Option<Store>,
}

/// A kind's kernel load: the registry it fills, and its two ops. The
/// load is the library type's `transit_version::Load` impl (D-239); the
/// generator checks that its constants are the line's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernelLoad {
    /// The registry's id, as `capability! { documents: [...] }` and the
    /// manifest's `documents` name it.
    pub id: &'static str,
    /// The JSON op (`{"op": "load_notes", ...}`).
    pub op: &'static str,
    /// The bulk op's code.
    pub code: u32,
}

/// How the store keeps a kind.
#[derive(Debug, Clone, Copy)]
pub struct Store {
    /// The envelope's kind, or a log's document kind.
    pub kind: &'static str,
    pub write: Write,
    /// Where it is written, in words a person reads in a refusal.
    pub route: &'static str,
    /// The pointers it heads (`*` ends a prefix).
    pub pointers: &'static [&'static str],
    /// The log that holds its records.
    pub log: Option<Log>,
    /// Its own write route, when it has one that its log's receive op
    /// answers: one op over the request's body.
    pub post: Option<Post>,
    /// The capability that checks a write, and its op.
    pub check: Option<(&'static str, &'static str)>,
    pub merge: Merge,
    pub read: Read,
}

/// The log that holds a kind's records, and its record kinds.
#[derive(Debug, Clone, Copy)]
pub struct Log {
    pub name: &'static str,
    /// The capability whose `receive` op writes the log (D-228): it runs
    /// a write's ops over the head and answers the entry. Absent while
    /// the server still sequences the log's writes itself.
    pub receive: Option<&'static str>,
    /// Each record kind, and the document field that holds its
    /// collection (an array of records with an `id`, or an object by id).
    pub records: &'static [(&'static str, &'static str)],
    /// Whether a page writes the log by intent (`POST /logs/<log>`).
    /// False: only the kind's routes write it (a freeze stores its
    /// network first; `POST /edits` is the edit document's protocol).
    pub intent: bool,
}

/// A kind's own write route, answered by its log's receive op: the
/// path, the op the request's body is the arguments of (an op of the
/// kind's checking capability, as the receive op takes it), the answer's
/// field that names the document after the write, and the body's fields
/// the answer repeats.
#[derive(Debug, Clone, Copy)]
pub struct Post {
    pub path: &'static str,
    pub op: &'static str,
    pub answer: &'static str,
    pub echo: &'static [&'static str],
}

/// How a kind is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Write {
    /// `POST /pointers/<name>`, one of `pointers`, checked by `module`'s
    /// `validate` op, and stored as the module answers it.
    Pointer,
    /// Its own route (`route`), which runs its module.
    Route,
    /// The server alone: an import, a release, or the manifest.
    Server,
}

/// Who reads a kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Read {
    /// Anyone, by hash (`/blobs/<hash>`; D-152).
    Open,
    /// Never served by hash. A client reads only its own projection.
    Restricted,
    /// A caller with the audit rule (a log's entries name their writers).
    Audit,
}

/// How two writes of a kind meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Merge {
    /// The whole document, by CAS on the head the writer read.
    Replace,
    /// Last writer wins for each record id (the edit document; D-56).
    Lww,
    /// Ops over the log's head; different records never conflict.
    Log,
    /// No writer but the server.
    None,
}

impl Declaration {
    /// A kind with no load and no store: a document a page keeps, or one
    /// that crosses only inside an op.
    const fn new(name: &'static str, tag: &'static str, library: &'static str, owner: &'static str) -> Self {
        Declaration { name, tag, library, owner, load: None, store: None }
    }

    /// The kind's kernel load.
    const fn loaded(mut self, id: &'static str, op: &'static str, code: u32) -> Self {
        self.load = Some(KernelLoad { id, op, code });
        self
    }

    /// How the store keeps the kind.
    const fn stored(
        mut self,
        kind: &'static str,
        write: Write,
        route: &'static str,
        pointers: &'static [&'static str],
        merge: Merge,
        read: Read,
    ) -> Self {
        self.store = Some(Store { kind, write, route, pointers, log: None, post: None, check: None, merge, read });
        self
    }

    /// The capability that checks a write of the stored kind, and its op.
    const fn checked(mut self, module: &'static str, op: &'static str) -> Self {
        if let Some(store) = &mut self.store {
            store.check = Some((module, op));
        }
        self
    }

    /// The capability whose `receive` op writes the stored kind's log.
    const fn received(mut self, capability: &'static str) -> Self {
        if let Some(Store { log: Some(log), .. }) = &mut self.store {
            log.receive = Some(capability);
        }
        self
    }

    /// The stored kind's log takes no write by intent: only its routes
    /// write it.
    const fn no_intent(mut self) -> Self {
        if let Some(Store { log: Some(log), .. }) = &mut self.store {
            log.intent = false;
        }
        self
    }

    /// The stored kind's own write route.
    const fn posted(mut self, path: &'static str, op: &'static str, answer: &'static str, echo: &'static [&'static str]) -> Self {
        if let Some(store) = &mut self.store {
            store.post = Some(Post { path, op, answer, echo });
        }
        self
    }

    /// The log that holds the stored kind's records.
    const fn logged(mut self, name: &'static str, records: &'static [(&'static str, &'static str)]) -> Self {
        if let Some(store) = &mut self.store {
            store.log = Some(Log { name, receive: None, records, intent: true });
        }
        self
    }
}

use Merge::{Lww, Replace};
use Read::{Audit, Open, Restricted};
use Write::{Pointer, Route, Server};

/// Every kind of document. The kernel's load codes are a wire contract:
/// a code is never reused (the network's is 256, the registry ops are
/// 257 and 258, and `about` is 268).
pub static KINDS: &[Declaration] = &[
    // The network and what an import gives with it.
    Declaration::new("network", "network", "transit_network::Network", "transit-network")
        .loaded("network", "load_network", 256)
        .stored("network_document", Route, "POST /plans/freeze, POST /plans; the server's import", &["plan.*", "network"], Replace, Open)
        .checked("network", "publishable"),
    Declaration::new("boundary", "boundary", "transit_codec::Boundary", "transit-codec")
        .loaded("boundary", "load_boundary", 261)
        .stored("boundary", Server, "the server's import of the GTFS zip", &["boundary"], Merge::None, Open),
    Declaration::new("feed", "", "transit_codec::Feed", "")
        .loaded("feed", "load_feed", 266)
        .stored("gtfs_feed", Server, "the server's -feed", &["source.feed"], Merge::None, Open),
    Declaration::new("plan_set", "plan_set", "transit_network::PlanSet", "transit-network"),
    // The operator's day.
    Declaration::new("operator_edits", "edit_records", "transit_sync::EditDocument", "transit-sync")
        .loaded("edits", "load_edits", 259)
        .stored("edit_records", Route, "POST /edits", &["edits"], Lww, Open)
        .checked("sync", "sync_edits")
        .logged("disruptions", &[("edit_record", "records")])
        .received("sync")
        .no_intent(),
    Declaration::new("live", "live", "transit_live::LiveDay", "transit-live").loaded("live", "load_live", 260),
    Declaration::new("rt", "", "transit_rt::RtOverview", "").loaded("rt", "load_rt", 267),
    // Streets and walking.
    Declaration::new("streets", "streets", "transit_streets::Streets", "transit-streets")
        .loaded("streets", "load_streets", 263)
        .stored("streets", Server, "the server's import of the street layer", &["streets"], Merge::None, Open),
    Declaration::new("street_layer", "", "", "").stored("street_layer", Server, "the server's -streets", &["source.streets"], Merge::None, Open),
    Declaration::new("walks", "walks", "transit_streets::walking::WalkTable", "transit-streets")
        .loaded("walks", "load_walks", 262)
        .stored("walks", Server, "the server's walk table", &["walks"], Merge::None, Open),
    Declaration::new("layer", "", "transit_analysis::WeightedLayer", "").loaded("layer", "load_layer", 265),
    // A rider's own documents.
    Declaration::new("watchlist", "watchlist", "transit_rider::watchlist::Watchlist", "transit-rider").loaded("watchlist", "load_watchlist", 264),
    Declaration::new("recent_places", "recent_places", "transit_rider::device::RecentPlaces", "transit-rider"),
    Declaration::new("live_feeds", "live_feeds", "transit_rider::device::LiveFeeds", "transit-rider"),
    // The runs layer.
    Declaration::new("work_rules", "work_rules", "transit_runs::WorkRules", "transit-runs").loaded("work_rules", "load_work_rules", 269),
    // The agency's record, in its logs.
    Declaration::new("stops", "stops", "transit_stops::Stops", "transit-stops")
        .loaded("stops", "load_stops", 270)
        .stored("stops", Route, "POST /stops, POST /logs/stops", &["stops"], Merge::Log, Open)
        .checked("stops", "validate")
        .logged("stops", &[("stop", "stops")])
        .received("stops")
        .posted("/stops", "validate", "stops_hash", &[]),
    Declaration::new("stop_requests", "stop_requests", "transit_requests::StopRequests", "transit-requests")
        .loaded("stop_requests", "load_stop_requests", 271)
        .stored("stop_requests", Route, "POST /stop-requests, POST /logs/stops", &["stop_requests"], Merge::Log, Open)
        .checked("requests", "validate")
        .logged("stops", &[("stop_request", "requests")])
        .received("stops")
        .posted("/stop-requests", "validate", "requests_hash", &[]),
    Declaration::new("service_alerts", "service_alerts", "transit_alerts::ServiceAlerts", "transit-alerts")
        .loaded("service_alerts", "load_service_alerts", 272)
        .stored("service_alerts", Route, "POST /service-alerts", &[], Merge::Log, Open)
        .checked("alerts", "validate")
        .logged("alerts", &[("service_alert", "alerts")])
        .received("alerts")
        .posted("/service-alerts", "validate", "alerts_hash", &[]),
    Declaration::new("alert_templates", "alert_templates", "transit_alerts::AlertTemplates", "transit-alerts")
        .loaded("alert_templates", "load_alert_templates", 273)
        .stored("alert_templates", Route, "POST /alert-templates", &[], Merge::Log, Open)
        .checked("alerts", "validate_templates")
        .logged("alerts", &[("alert_template", "templates")])
        .received("alerts")
        .posted("/alert-templates", "validate_templates", "templates_hash", &[]),
    Declaration::new("notes", "notes", "transit_notes::Notes", "transit-notes")
        .loaded("notes", "load_notes", 274)
        .stored("notes", Route, "POST /notes", &[], Merge::Log, Open)
        .checked("notes", "validate")
        .logged("notes", &[("note", "notes")])
        .received("notes")
        .posted("/notes", "write", "notes_hash", &["id"]),
    Declaration::new("plans", "plans", "transit_plans::PublishedPlans", "transit-plans")
        .stored("plans", Route, "POST /plans, POST /plans/adopt, POST /plans/retire", &["plans"], Merge::Log, Open)
        .checked("plans", "adopt")
        .logged("plans", &[("pick", "plans")])
        .received("plans")
        .no_intent(),
    Declaration::new("frozen_versions", "frozen_versions", "transit_plans::FrozenVersions", "transit-plans")
        .stored("frozen_versions", Route, "POST /plans/freeze, POST /plans", &[], Merge::Log, Open)
        .checked("plans", "freeze")
        .logged("plans", &[("frozen_version", "versions")])
        .received("plans")
        .no_intent(),
    Declaration::new("log_entry", "log_entry", "transit_log::Entry", "transit-log").stored("log_entry", Route, "the log's own writes", &[], Merge::None, Audit),
    Declaration::new("log_checkpoint", "log_checkpoint", "transit_log::Checkpoint", "transit-log")
        .stored("log_checkpoint", Server, "the server's fold", &[], Merge::None, Audit),
    // Sign-in.
    Declaration::new("policy", "policy", "transit_authz::Policy", "transit-authz")
        .stored("policy", Pointer, "POST /pointers/policy", &["policy"], Replace, Restricted)
        .checked("authz", "validate"),
    // The server's own documents.
    Declaration::new("derivation", "derivation", "", "server").stored("derivation", Server, "the server's imports", &["derived.*"], Merge::None, Open),
    Declaration::new("manifest", "", "", "").stored("manifest", Server, "the server's publishing", &["manifest"], Merge::None, Open),
    Declaration::new("build", "build", "", "server").stored("build", Route, "the server's -dist, and POST /builds", &["build", "build.*"], Replace, Open),
    Declaration::new("build_file", "", "", "").stored("build_file", Route, "the server's -dist", &[], Merge::None, Open),
    Declaration::new("kind_table", "kind_table", "kind_table::KindTable", "cap-abi")
        .stored("kind_table", Server, "the server's setup of the store", &[], Merge::None, Open),
    Declaration::new("store_layout", "store_layout", "", "server").stored("store_layout", Server, "the server's setup of the store", &[], Merge::None, Open),
];

/// The line whose tag is `tag`.
pub fn by_tag(tag: &str) -> Option<&'static Declaration> {
    KINDS.iter().find(|k| !k.tag.is_empty() && k.tag == tag)
}

/// The line whose kernel load fills the registry `id`.
pub fn by_load(id: &str) -> Option<&'static Declaration> {
    KINDS.iter().find(|k| k.load.is_some_and(|l| l.id == id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn each_name_tag_load_store_kind_and_code_is_one_line() {
        let mut seen = BTreeSet::new();
        for k in KINDS {
            assert!(seen.insert(("name", k.name.to_string())), "two lines name {}", k.name);
            if !k.tag.is_empty() {
                assert!(seen.insert(("tag", k.tag.to_string())), "two lines carry the tag {}", k.tag);
            }
            if let Some(l) = k.load {
                assert!(seen.insert(("load", l.id.to_string())), "two lines fill {}", l.id);
                assert!(seen.insert(("op", l.op.to_string())), "two lines load by {}", l.op);
                assert!(seen.insert(("code", l.code.to_string())), "two lines load by code {}", l.code);
                assert!(l.code >= crate::kernel::KERNEL_OPS, "{}: a load is a kernel op", k.name);
                assert!(![257, 258, crate::kernel::OP_ABOUT].contains(&l.code), "{}: {} is the registry's or about's", k.name, l.code);
            }
            if let Some(s) = k.store {
                assert!(seen.insert(("store", s.kind.to_string())), "two lines store {}", s.kind);
            }
        }
    }

    #[test]
    fn a_kind_s_own_route_is_answered_by_its_log_s_receive_op() {
        for k in KINDS {
            let Some(s) = k.store else { continue };
            if let Some(post) = s.post {
                assert!(s.log.is_some_and(|l| l.receive.is_some()), "{}: {} needs a receive op", k.name, post.path);
                assert!(s.route.contains(post.path), "{}: the route's words name {}", k.name, post.path);
            }
        }
    }

    #[test]
    fn a_pointer_kind_is_checked_and_a_logged_kind_merges_by_record() {
        for k in KINDS {
            let Some(s) = k.store else { continue };
            if s.write == Pointer {
                assert!(s.check.is_some(), "{}: the server refuses a pointer write that no module checks", k.name);
            }
            if let Some(log) = s.log {
                assert!(matches!(s.merge, Merge::Log | Lww), "{}: a log's records merge one by one", k.name);
                // Every kind of one log is written by one receive op: one
                // entry can change records of each (D-232).
                let mut others = KINDS.iter().filter_map(|o| o.store?.log).filter(|o| o.name == log.name);
                assert!(others.clone().all(|o| o.receive == log.receive), "the {} log has one receive op", log.name);
                // A log takes a page's intent write, or does not, whole.
                assert!(others.all(|o| o.intent == log.intent), "the {} log's kinds agree on intent", log.name);
            }
        }
    }
}
