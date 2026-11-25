//! A kind exists only if it has a line in [`KINDS`]. The build does not
//! discover kinds by link-time registration or by a scan of the crates. The
//! schema of a kind declares the fields that name other records
//! (`x-references`, D-232). The line does not.
//!
//! The kind table (`crates/cap-abi/kinds.json`, D-202) is generated from this
//! list and the locked schemas. It is checked in, so a new kind shows in the
//! diff of a review. gen-manifest stops while the table differs from the
//! list, and `just schema-lock` writes the table again.
//!
//! No module reads the list. A kind costs a module nothing until a
//! capability names it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy)]
pub struct Declaration {
    /// The registry's documents index, the manifest's `speaks`, and the
    /// target of a content-hash reference all use this name.
    pub name: &'static str,
    /// The tag inside a document names the schema. It is empty for a kind
    /// that has no schema of ours, such as a GTFS zip or a file of a build.
    pub tag: &'static str,
    /// Empty for a kind that the Go host owns or that has no type.
    pub library: &'static str,
    /// The crate that keeps the schema lock (`crates/<owner>/locks/`).
    /// `server` means the Go host's lock (`server/schemas/`).
    pub owner: &'static str,
    pub load: Option<KernelLoad>,
    pub store: Option<Store>,
}

/// The generator checks that the constants of the library type's
/// `transit_version::Load` impl equal these (D-239).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernelLoad {
    pub id: &'static str,
    pub op: &'static str,
    pub code: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Store {
    pub kind: &'static str,
    pub write: Write,
    /// Words that a person reads in a refusal.
    pub route: &'static str,
    /// A `*` ends a prefix.
    pub pointers: &'static [&'static str],
    pub log: Option<Log>,
    pub post: Option<Post>,
    pub work_order: Option<WorkOrder>,
    pub pick: Option<Pick>,
    /// The checking capability's op that records the differences between
    /// an imported feed and the kind, through the kind's log (D-150).
    pub feed_events: Option<&'static str>,
    /// The capability that checks a write, and its op.
    pub check: Option<(&'static str, &'static str)>,
    pub merge: Merge,
    pub read: Read,
}

#[derive(Debug, Clone, Copy)]
pub struct Log {
    pub name: &'static str,
    /// The capability whose `receive` op writes the log (D-228). Absent
    /// while the server still sequences the log's writes itself.
    pub receive: Option<&'static str>,
    /// Each record kind, with the document field that holds its collection:
    /// an array of records with an `id`, or an object keyed by id.
    pub records: &'static [(&'static str, &'static str)],
    /// True when a page writes the log by intent (`POST /logs/<log>`). When
    /// it is false, only the routes of the kind write the log. A freeze
    /// stores its network first, and `POST /edits` is the protocol of the
    /// edit document.
    pub intent: bool,
}

/// The route runs one op of the kind's checking capability over the body of
/// the request. The receive op of the kind's log answers it.
#[derive(Debug, Clone, Copy)]
pub struct Post {
    pub path: &'static str,
    pub op: &'static str,
    /// The field of the answer that names the document after the write.
    pub answer: &'static str,
    pub echo: &'static [&'static str],
}

/// The kind's records are sent to the work order system (D-165) through
/// this route. The checking capability's `call` op answers what to send,
/// the server makes the call, and its `record` op, through the kind's
/// log, writes the answer (D-228).
#[derive(Debug, Clone, Copy)]
pub struct WorkOrder {
    pub method: &'static str,
    pub path: &'static str,
    pub call: &'static str,
    pub record: &'static str,
}

/// A pick reads the kind. An adoption registers the kind's head and names
/// the seq of its log it read as the `seq` argument (D-191). A pick's
/// export runs the checking capability's `export` op over the network and
/// the head (D-151).
#[derive(Debug, Clone, Copy)]
pub struct Pick {
    pub seq: &'static str,
    pub export: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Write {
    /// `POST /pointers/<name>`, where the name is one of `pointers`. The
    /// `validate` op of the `module` checks the write, and the store
    /// keeps what the module answers.
    Pointer,
    /// The kind has its own route (`route`), which runs its module.
    Route,
    /// Only the server writes the kind, in an import, a release, or the
    /// manifest.
    Server,
}

/// Who reads a kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Read {
    /// Anyone reads the kind by hash (`/blobs/<hash>`; D-152).
    Open,
    /// The server does not serve the kind by hash. A client reads only
    /// its own projection.
    Restricted,
    /// A caller with the audit rule reads the kind. The entries of a log
    /// name their writers.
    Audit,
}

/// How two writes of a kind merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Merge {
    /// The write replaces the whole document, by CAS on the head that the
    /// writer read.
    Replace,
    /// The last writer wins for each record id, as in the edit document
    /// (D-56).
    Lww,
    /// The writes are ops over the head of the log. Writes to different
    /// records do not conflict.
    Log,
    /// Only the server writes the kind.
    None,
}

impl Declaration {
    pub const fn new(name: &'static str, tag: &'static str, library: &'static str, owner: &'static str) -> Self {
        Declaration { name, tag, library, owner, load: None, store: None }
    }

    pub const fn loaded(mut self, id: &'static str, op: &'static str, code: u32) -> Self {
        self.load = Some(KernelLoad { id, op, code });
        self
    }

    pub const fn stored(
        mut self,
        kind: &'static str,
        write: Write,
        route: &'static str,
        pointers: &'static [&'static str],
        merge: Merge,
        read: Read,
    ) -> Self {
        self.store = Some(Store { kind, write, route, pointers, log: None, post: None, work_order: None, pick: None, feed_events: None, check: None, merge, read });
        self
    }

    pub const fn checked(mut self, module: &'static str, op: &'static str) -> Self {
        if let Some(store) = &mut self.store {
            store.check = Some((module, op));
        }
        self
    }

    pub const fn received(mut self, capability: &'static str) -> Self {
        if let Some(Store { log: Some(log), .. }) = &mut self.store {
            log.receive = Some(capability);
        }
        self
    }

    pub const fn no_intent(mut self) -> Self {
        if let Some(Store { log: Some(log), .. }) = &mut self.store {
            log.intent = false;
        }
        self
    }

    pub const fn posted(mut self, path: &'static str, op: &'static str, answer: &'static str, echo: &'static [&'static str]) -> Self {
        if let Some(store) = &mut self.store {
            store.post = Some(Post { path, op, answer, echo });
        }
        self
    }

    pub const fn work_orders(mut self, method: &'static str, path: &'static str, call: &'static str, record: &'static str) -> Self {
        if let Some(store) = &mut self.store {
            store.work_order = Some(WorkOrder { method, path, call, record });
        }
        self
    }

    pub const fn picked(mut self, seq: &'static str, export: &'static str) -> Self {
        if let Some(store) = &mut self.store {
            store.pick = Some(Pick { seq, export });
        }
        self
    }

    pub const fn feed_events(mut self, op: &'static str) -> Self {
        if let Some(store) = &mut self.store {
            store.feed_events = Some(op);
        }
        self
    }

    pub const fn logged(mut self, name: &'static str, records: &'static [(&'static str, &'static str)]) -> Self {
        if let Some(store) = &mut self.store {
            store.log = Some(Log { name, receive: None, records, intent: true });
        }
        self
    }
}

use Merge::{Lww, Replace};
use Read::{Audit, Open, Restricted};
use Write::{Pointer, Route, Server};

/// The load codes are a wire contract, so a code is never reused. The
/// registry ops are 257 and 258, and `about` is 268.
pub static KINDS: &[Declaration] = &[
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
    Declaration::new("operator_edits", "edit_records", "transit_sync::EditDocument", "transit-sync")
        .loaded("edits", "load_edits", 259)
        .stored("edit_records", Route, "POST /edits", &["edits"], Lww, Open)
        .checked("sync", "sync_edits")
        .logged("disruptions", &[("edit_record", "records")])
        .received("sync")
        .no_intent(),
    Declaration::new("live", "live", "transit_live::LiveDay", "transit-live").loaded("live", "load_live", 260),
    Declaration::new("rt", "", "transit_rt::RtOverview", "").loaded("rt", "load_rt", 267),
    Declaration::new("streets", "streets", "transit_streets::Streets", "transit-streets")
        .loaded("streets", "load_streets", 263)
        .stored("streets", Server, "the server's import of the street layer", &["streets"], Merge::None, Open),
    Declaration::new("street_layer", "", "", "").stored("street_layer", Server, "the server's -streets", &["source.streets"], Merge::None, Open),
    Declaration::new("walks", "walks", "transit_streets::walking::WalkTable", "transit-streets")
        .loaded("walks", "load_walks", 262)
        .stored("walks", Server, "the server's walk table", &["walks"], Merge::None, Open),
    Declaration::new("layer", "", "transit_analysis::WeightedLayer", "").loaded("layer", "load_layer", 265),
    Declaration::new("watchlist", "watchlist", "transit_rider::watchlist::Watchlist", "transit-rider").loaded("watchlist", "load_watchlist", 264),
    Declaration::new("recent_places", "recent_places", "transit_rider::device::RecentPlaces", "transit-rider"),
    Declaration::new("live_feeds", "live_feeds", "transit_rider::device::LiveFeeds", "transit-rider"),
    Declaration::new("work_rules", "work_rules", "transit_runs::WorkRules", "transit-runs").loaded("work_rules", "load_work_rules", 269),
    Declaration::new("stops", "stops", "transit_stops::Stops", "transit-stops")
        .loaded("stops", "load_stops", 270)
        .stored("stops", Route, "POST /stops, POST /logs/stops", &["stops"], Merge::Log, Open)
        .checked("stops", "validate")
        .logged("stops", &[("stop", "stops")])
        .received("stops")
        .posted("/stops", "validate", "stops_hash", &[])
        .picked("stops_seq", "export_pick")
        .feed_events("import_events"),
    Declaration::new("stop_requests", "stop_requests", "transit_requests::StopRequests", "transit-requests")
        .loaded("stop_requests", "load_stop_requests", 271)
        .stored("stop_requests", Route, "POST /stop-requests, POST /logs/stops", &["stop_requests"], Merge::Log, Open)
        .checked("requests", "validate")
        .logged("stops", &[("stop_request", "requests")])
        .received("stops")
        .posted("/stop-requests", "validate", "requests_hash", &[])
        .work_orders("POST", "/stop-requests/work-order", "work_order_call", "record_work_order"),
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
    Declaration::new("policy", "policy", "transit_authz::Policy", "transit-authz")
        .stored("policy", Pointer, "POST /pointers/policy", &["policy"], Replace, Restricted)
        .checked("authz", "validate"),
    Declaration::new("derivation", "derivation", "", "server").stored("derivation", Server, "the server's imports", &["derived.*"], Merge::None, Open),
    Declaration::new("manifest", "", "", "").stored("manifest", Server, "the server's publishing", &["manifest"], Merge::None, Open),
    Declaration::new("build", "build", "", "server").stored("build", Route, "the server's -dist, and POST /builds", &["build", "build.*"], Replace, Open),
    Declaration::new("build_file", "", "", "").stored("build_file", Route, "the server's -dist", &[], Merge::None, Open),
    Declaration::new("kind_table", "kind_table", "kind_table::KindTable", "cap-abi")
        .stored("kind_table", Server, "the server's setup of the store", &[], Merge::None, Open),
    Declaration::new("store_layout", "store_layout", "", "server").stored("store_layout", Server, "the server's setup of the store", &[], Merge::None, Open),
];

pub fn by_tag(tag: &str) -> Option<&'static Declaration> {
    KINDS.iter().find(|k| !k.tag.is_empty() && k.tag == tag)
}

pub fn by_load(id: &str) -> Option<&'static Declaration> {
    KINDS.iter().find(|k| k.load.is_some_and(|l| l.id == id))
}

/// What makes a kind list wrong: two lines with one name, tag, load,
/// code, or store kind; a load that is not a kernel op; a route with no
/// receive op; a pointer kind that no module checks; a log whose kinds
/// disagree. Empty for a list the server and the generator can take.
pub fn problems(kinds: &[Declaration]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut once = |what: &str, value: String, line: &str| {
        if !seen.insert((what.to_string(), value.clone())) {
            out.push(format!("{line}: another line has the {what} {value}"));
        }
    };
    for k in kinds {
        once("name", k.name.to_string(), k.name);
        if !k.tag.is_empty() {
            once("tag", k.tag.to_string(), k.name);
        }
        if let Some(l) = k.load {
            once("load", l.id.to_string(), k.name);
            once("op", l.op.to_string(), k.name);
            once("code", l.code.to_string(), k.name);
        }
        if let Some(s) = k.store {
            once("store kind", s.kind.to_string(), k.name);
        }
    }
    for k in kinds {
        if let Some(l) = k.load {
            if l.code < crate::kernel::KERNEL_OPS || [257, 258, crate::kernel::OP_ABOUT].contains(&l.code) {
                out.push(format!("{}: the load code {} is not a free kernel op", k.name, l.code));
            }
        }
        let Some(s) = k.store else { continue };
        if let Some(post) = s.post {
            if !s.log.is_some_and(|l| l.receive.is_some()) {
                out.push(format!("{}: {} needs a receive op", k.name, post.path));
            }
            if !s.route.contains(post.path) {
                out.push(format!("{}: the route's words do not name {}", k.name, post.path));
            }
        }
        if s.write == Pointer && s.check.is_none() {
            out.push(format!("{}: the server refuses a pointer write that no module checks", k.name));
        }
        if let Some(log) = s.log {
            if !matches!(s.merge, Merge::Log | Lww) {
                out.push(format!("{}: a log's records merge one by one", k.name));
            }
            // One entry can change records of each kind (D-232).
            let others: Vec<Log> = kinds.iter().filter_map(|o| o.store?.log).filter(|o| o.name == log.name).collect();
            if !others.iter().all(|o| o.receive == log.receive && o.intent == log.intent) {
                out.push(format!("the {} log's kinds disagree on its receive op or its intent", log.name));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kind_list_has_no_problems() {
        assert_eq!(problems(KINDS), Vec::<String>::new());
    }

    #[test]
    fn a_broken_list_has_each_problem() {
        let broken = [
            Declaration::new("a", "a", "", "").loaded("a", "load_a", 12).stored("a", Pointer, "POST /x", &["a"], Replace, Open),
            Declaration::new("a", "b", "", "").stored("b", Route, "POST /b", &[], Merge::Log, Open).logged("l", &[]).posted("/c", "v", "b_hash", &[]),
            Declaration::new("c", "c", "", "").stored("c", Route, "", &[], Replace, Open).logged("l", &[]).received("x"),
        ];
        let found = problems(&broken);
        for want in ["another line has the name a", "not a free kernel op", "needs a receive op", "do not name /c", "no module checks", "merge one by one", "disagree"] {
            assert!(found.iter().any(|p| p.contains(want)), "{want}: {found:?}");
        }
    }
}
