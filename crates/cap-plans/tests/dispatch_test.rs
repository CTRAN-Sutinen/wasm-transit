//! Native dispatch tests for the plan-chain capability — the seam
//! without wasm, over plan-set metadata. The chain's own laws
//! (resolution, windows, validity, order-insensitivity) are tested
//! at the library seam (transit-plans); here the module's contract
//! is pinned: the tagged wire form round-trips, and garbage or
//! bulk bytes are honest errors, never panics.

use cap_abi::{Base, Current};
use cap_plans::{dispatch, dispatch_bytes, dispatch_json, PlanInfo, Request, Response};

fn info(id: &str, name: &str, from: Option<&str>) -> PlanInfo {
    PlanInfo { id: id.into(), name: name.into(), effective_from: from.map(Into::into) }
}

fn resolved(plans: &[PlanInfo], d: &str) -> Option<String> {
    match dispatch(Request::Resolve { plans: plans.to_vec(), date: d.into() }).unwrap() {
        Response::Resolve { plan } => plan,
        _ => panic!("resolve"),
    }
}

#[test]
fn the_seam_answers_the_chain() {
    let chain = vec![
        info("spring", "Spring 2026", Some("2026-03-01")),
        info("fall", "Fall 2026", Some("2026-09-07")),
    ];
    assert_eq!(resolved(&chain, "2026-01-01"), None);
    assert_eq!(resolved(&chain, "2026-10-24"), Some("fall".into()));
    // Unversioned plans never resolve through the seam either.
    assert_eq!(resolved(&[info("", "imported", None)], "2026-10-24"), None);

    match dispatch(Request::Validate { plans: chain.clone() }).unwrap() {
        Response::Validate { issues } => assert!(issues.is_empty()),
        _ => panic!("validate"),
    }
    match dispatch(Request::Periods { plans: chain }).unwrap() {
        Response::Periods { periods } => {
            assert_eq!(periods.len(), 2);
            assert_eq!(periods[0].until.as_deref(), Some("2026-09-07"));
        }
        _ => panic!("periods"),
    }
}

#[test]
fn garbage_and_bulk_bytes_are_errors_not_panics() {
    let err = dispatch(Request::Resolve { plans: vec![], date: "2026-13-01".into() }).unwrap_err();
    assert!(err.contains("invalid date"), "{err}");

    let bad = dispatch_json(r#"{"op":"nope"}"#);
    assert!(bad.contains("invalid request"), "{bad}");
    let bad = dispatch_json(r#"{"op":"resolve","plans":[],"date":"not a date"}"#);
    assert!(bad.contains("error"), "{bad}");
    let bad = dispatch_bytes(0, b"whatever");
    assert!(bad.contains("unknown bulk op"), "the chain takes metadata inline: {bad}");

    // The tagged wire form round-trips.
    let ok = dispatch_json(r#"{"op":"validate","plans":[]}"#);
    assert!(ok.contains(r#""issues":[]"#), "{ok}");
}

/// Publishing through the seam: the module is the hash authority for
/// the published plan set — its answer carries the canonical bytes and
/// their hash, and a host stores exactly those (docs/FORMATS.md).
#[test]
fn publish_answers_the_canonical_document_and_its_hash() {
    use cap_plans::PublishedPlans;
    let fall = info("fall", "Fall 2026", Some("2026-09-07"));
    let h = format!("sha256:{}", "ab".repeat(32));
    let (document, document_hash, published) =
        match dispatch(Request::Publish { published: None, plan: fall.clone(), network_hash: h.clone(), today: "2026-08-01".into() }).unwrap() {
            Response::Publish { document: Some(d), document_hash: Some(dh), published: Some(p), issues } => {
                assert!(issues.is_empty());
                (d, dh, p)
            }
            other => panic!("publish: {other:?}"),
        };
    assert_eq!(document, cap_abi::canonical_bytes(&published).unwrap(), "the document is the canonical serialization");
    assert_eq!(document_hash, cap_abi::content_hash(document.as_bytes()), "the hash names those exact bytes");
    let reread: PublishedPlans = serde_json::from_str(&document).unwrap();
    assert_eq!(reread, published);

    // in_force answers the pick and its network.
    match dispatch(Request::InForce { published: Current(published.clone()), date: "2026-11-05".into() }).unwrap() {
        Response::InForce { plan: Some(p) } => assert_eq!((p.id.as_str(), p.network_hash.as_str()), ("fall", h.as_str())),
        other => panic!("in_force: {other:?}"),
    }
    match dispatch(Request::InForce { published: Current(published.clone()), date: "2026-01-01".into() }).unwrap() {
        Response::InForce { plan: None } => {}
        other => panic!("before the first pick nothing is in force: {other:?}"),
    }

    // in_use answers the pick in force and the picks still to come.
    match dispatch(Request::InUse { published: Current(published.clone()), today: "2026-11-05".into() }).unwrap() {
        Response::InUse { plans } => assert_eq!(plans.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["fall"]),
        other => panic!("in_use: {other:?}"),
    }

    // A refusal is an answer with its reasons, never a stored document.
    match dispatch(Request::Publish { published: Some(Base(published.clone())), plan: info("", "import", None), network_hash: h.clone(), today: "2026-08-01".into() }).unwrap() {
        Response::Publish { issues, document: None, document_hash: None, published: None } => assert!(!issues.is_empty()),
        other => panic!("refusal: {other:?}"),
    }

    // A set from a newer schema is refused naming both, not guessed at.
    let mut newer = published;
    newer.schema_version = 99;
    let err = dispatch(Request::InForce { published: Current(newer.clone()), date: "2026-11-05".into() }).unwrap_err();
    assert!(err.contains("plans v99") && err.contains("reads v1"), "{err}");
    let err = dispatch(Request::InUse { published: Current(newer), today: "2026-11-05".into() }).unwrap_err();
    assert!(err.contains("plans v99") && err.contains("reads v1"), "{err}");
}

/// Retiring through the seam (D-75): a future pick is retired and the
/// answer is the next canonical set; a pick that took effect is refused
/// with its reason. The host passes its date, as it does to publish.
#[test]
fn retire_answers_the_next_set_or_why_not() {
    let h = format!("sha256:{}", "cd".repeat(32));
    let published = match dispatch(Request::Publish {
        published: None,
        plan: info("spring", "Spring 2027", Some("2027-03-01")),
        network_hash: h,
        today: "2026-11-05".into(),
    })
    .unwrap()
    {
        Response::Publish { published: Some(p), .. } => p,
        other => panic!("publish: {other:?}"),
    };
    match dispatch(Request::Retire { published: Base(published.clone()), id: "spring".into(), today: "2026-11-05".into() }).unwrap() {
        Response::Retire { document: Some(d), document_hash: Some(dh), published: Some(p), issues } => {
            assert!(issues.is_empty());
            assert_eq!(dh, cap_abi::content_hash(d.as_bytes()));
            assert_eq!(p.plans[0].status, cap_plans::PickStatus::Retired);
        }
        other => panic!("retire: {other:?}"),
    }
    match dispatch(Request::Retire { published: Base(published), id: "spring".into(), today: "2027-03-01".into() }).unwrap() {
        Response::Retire { issues, document: None, .. } => assert!(!issues.is_empty()),
        other => panic!("a pick that took effect is refused: {other:?}"),
    }
    // A publish dated before the host's today is refused, not stored.
    match dispatch(Request::Publish {
        published: None,
        plan: info("old", "Old", Some("2026-01-01")),
        network_hash: format!("sha256:{}", "ef".repeat(32)),
        today: "2026-11-05".into(),
    })
    .unwrap()
    {
        Response::Publish { issues, document: None, .. } => assert!(!issues.is_empty()),
        other => panic!("a past pick: {other:?}"),
    }
    assert!(dispatch_json(r#"{"op":"retire","published":{"schema_version":1,"plans":[]},"id":"x","today":"nope"}"#).contains("error"));
}

fn hash(n: u8) -> String {
    format!("sha256:{}", format!("{n:02x}").repeat(32))
}

fn answer(req: serde_json::Value) -> serde_json::Value {
    let out: serde_json::Value = serde_json::from_str(&dispatch_json(&req.to_string())).unwrap();
    out.get("ok").cloned().unwrap_or_else(|| panic!("{req}: {out}"))
}

/// A held record as the log's fold has it.
fn held(kinds: &serde_json::Value, kind: &str) -> (serde_json::Value, Option<serde_json::Value>) {
    let mut records = serde_json::Map::new();
    let mut own = None;
    for k in kinds.as_array().unwrap() {
        for (id, value) in k["records"].as_object().unwrap() {
            let h = serde_json::json!({"version": k["version"], "value": value});
            if k["kind"] == kind {
                records.insert(id.clone(), h);
            } else if id == "document" && (k["kind"] == "plans" && kind == "pick" || k["kind"] == "frozen_versions" && kind == "frozen_version") {
                own = Some(h);
            }
        }
    }
    (serde_json::Value::Object(records), own)
}

#[test]
fn a_freeze_then_an_adoption_through_the_seam() {
    let plan = serde_json::json!({"id": "fall", "name": "Fall 2026", "effective_from": "2026-09-07"});
    let frozen = answer(serde_json::json!({"op": "freeze", "frozen": null, "plan": plan, "network_hash": hash(1), "purpose": "proposal for public comment"}));
    assert!(frozen["issues"].as_array().unwrap().is_empty());
    let refused = answer(serde_json::json!({"op": "adopt", "published": null, "frozen": frozen["frozen"], "network_hash": hash(2), "today": "2026-01-01"}));
    assert_eq!(refused["issues"][0]["kind"], "not_frozen", "only a frozen version is adopted: {refused}");
    assert!(refused["document"].is_null());
    let adopted = answer(serde_json::json!({"op": "adopt", "published": null, "frozen": frozen["frozen"], "network_hash": hash(1), "today": "2026-01-01", "stops_seq": 7}));
    assert_eq!(adopted["published"]["plans"][0]["stops_seq"], 7);
    assert_eq!(adopted["published"]["plans"][0]["network_hash"], hash(1));
    assert_eq!(adopted["next"][0]["kind"], "pick");
}

#[test]
fn the_documents_round_trip_through_the_log_records() {
    let frozen = answer(serde_json::json!({"op": "freeze", "frozen": null, "plan": {"id": "fall", "name": "Fall", "effective_from": "2026-09-07"}, "network_hash": hash(1), "purpose": "board packet"}));
    let frozen = answer(serde_json::json!({"op": "freeze", "frozen": frozen["frozen"], "plan": {"id": "spring", "name": "Spring", "effective_from": "2026-03-01"}, "network_hash": hash(2), "purpose": "proposal"}));
    let mut set = serde_json::Value::Null;
    for n in [1u8, 2] {
        set = answer(serde_json::json!({"op": "adopt", "published": set, "frozen": frozen["frozen"], "network_hash": hash(n), "today": "2026-01-01"}))["published"].clone();
    }
    let records = answer(serde_json::json!({"op": "log_records", "published": set, "frozen": frozen["frozen"]}))["next"].clone();
    let (picks, own) = held(&records, "pick");
    let plans = answer(serde_json::json!({"op": "log_document", "kind": "plans", "records": picks, "document": own}));
    let direct = answer(serde_json::json!({"op": "log_records", "published": set}));
    assert!(!direct["next"].as_array().unwrap().is_empty());
    let reread: serde_json::Value = serde_json::from_str(plans["document"].as_str().unwrap()).unwrap();
    assert_eq!(reread, set, "the set in its one order, from its records");
    let (versions, own) = held(&records, "frozen_version");
    let back = answer(serde_json::json!({"op": "log_document", "kind": "frozen_versions", "records": versions, "document": own}));
    assert_eq!(back["document"].as_str().unwrap(), cap_abi::canonical_bytes(&serde_json::from_value::<cap_plans::FrozenVersions>(frozen["frozen"].clone()).unwrap()).unwrap());
    assert!(dispatch_json(r#"{"op":"log_document","kind":"other","records":{}}"#).contains("error"));
}

#[test]
fn the_timeline_answers_rows_the_pick_in_force_and_the_chain() {
    let published = serde_json::json!({"kind": "plans", "schema_version": 1, "plans": [
        {"id": "fall", "name": "Fall", "effective_from": "2026-10-19", "network_hash": "sha256:00", "status": "published"},
        {"id": "winter", "name": "Winter", "effective_from": "2027-01-04", "network_hash": "sha256:00", "status": "published"}]});
    let ask = |request: serde_json::Value| -> serde_json::Value { serde_json::from_str(&dispatch_json(&request.to_string())).unwrap() };
    let out = ask(serde_json::json!({"op": "timeline", "today": "2026-11-13", "published": published, "plans": [
        {"key": "plan-fall", "id": "fall", "name": "Fall", "effective_from": "2026-10-19"},
        {"key": "plan-draft", "id": "spring", "name": "Draft", "effective_from": "2027-03-01"}]}));
    let out = &out["ok"];
    assert_eq!(out["rows"].as_array().unwrap().len(), 3);
    assert_eq!(out["rows"][0]["published"], true);
    assert_eq!(out["rows"][1]["published"], false);
    assert!(out["rows"][2].get("key").is_none() && out["rows"][2]["loaded"] == false);
    assert_eq!(out["in_force"], "fall");
    assert_eq!(out["periods"].as_array().unwrap().len(), 3);
    // without a set, nothing is in force
    let none = ask(serde_json::json!({"op": "timeline", "today": "2026-11-13", "plans": []}));
    assert_eq!(none["ok"]["in_force"], serde_json::Value::Null);
    let bad = ask(serde_json::json!({"op": "timeline", "today": "2026-13-01", "plans": []}));
    assert!(bad.get("err").is_some() || bad.get("error").is_some(), "{bad}");
}

// ---------------------------------------------------------------------
// receive: the plans log's write, whole (D-228)
// ---------------------------------------------------------------------

fn header(seq: u64, key: Option<&str>) -> serde_json::Value {
    let mut intent = serde_json::json!({ "op": "POST /plans" });
    if let Some(k) = key {
        intent["key"] = serde_json::json!(k);
    }
    serde_json::json!({ "log": "plans", "seq": seq, "writer": "planner-1", "at_ms": 2_000_000, "module": "cap-plans@test", "intent": intent })
}

fn entry_kinds(got: &serde_json::Value) -> Vec<String> {
    let entry: serde_json::Value = serde_json::from_str(got["entry"].as_str().unwrap_or_else(|| panic!("an entry: {got}"))).unwrap();
    let mut kinds: Vec<String> = entry["changes"].as_array().unwrap().iter().map(|c| c["kind"].as_str().unwrap().to_string()).collect();
    kinds.sort();
    kinds.dedup();
    kinds
}

fn fall() -> serde_json::Value {
    serde_json::json!({"id": "fall", "name": "Fall", "effective_from": "2026-09-07"})
}

#[test]
fn receive_freezes_and_adopts_in_one_entry() {
    let got = answer(serde_json::json!({ "op": "receive", "write": { "header": header(1, Some("k1")) }, "ops": [
        {"cap": "plans", "op": "freeze", "args": {"plan": fall(), "network_hash": hash(1), "purpose": "the published pick"}},
        {"cap": "plans", "op": "adopt", "args": {"network_hash": hash(1), "today": "2026-01-01", "stops_seq": 4}}
    ] }));
    assert_eq!(entry_kinds(&got), vec!["frozen_version", "frozen_versions", "pick", "plans"], "{got}");
    assert!(got["heads"]["plans"].is_string() && got["heads"]["frozen_versions"].is_string(), "{got}");
}

#[test]
fn receive_answers_a_refused_adoption_with_its_issues_and_no_entry() {
    let got = answer(serde_json::json!({ "op": "receive", "write": { "header": header(1, None) }, "ops": [
        {"cap": "plans", "op": "adopt", "args": {"network_hash": hash(2), "today": "2026-01-01"}}
    ] }));
    assert_eq!(got["refused"]["op"], 0, "{got}");
    assert_eq!(got["refused"]["refusals"][0]["kind"], "not_frozen", "{got}");
    assert!(got.get("entry").is_none());
    let bad = answer(serde_json::json!({ "op": "receive", "write": { "header": header(1, None) }, "ops": [{"cap": "stops", "op": "add_event"}] }));
    assert!(bad["failed"]["error"].as_str().unwrap().contains("not a write to the plans log"), "{bad}");
}

#[test]
fn receive_retires_over_the_documents_it_is_given() {
    let frozen = answer(serde_json::json!({"op": "freeze", "frozen": null, "plan": fall(), "network_hash": hash(1), "purpose": "p"}))["frozen"].clone();
    let set = answer(serde_json::json!({"op": "adopt", "published": null, "frozen": frozen, "network_hash": hash(1), "today": "2026-01-01"}))["published"].clone();
    let got = answer(serde_json::json!({ "op": "receive", "documents": {"plans": set, "frozen_versions": frozen},
        "write": { "header": header(1, None) },
        "ops": [{"cap": "plans", "op": "retire", "args": {"id": "fall", "today": "2026-01-01"}}] }));
    assert_eq!(entry_kinds(&got), vec!["pick", "plans"], "only the published set is written: {got}");
    assert!(got["heads"].get("frozen_versions").is_none(), "{got}");
}

#[test]
fn receive_migrate_freezes_each_published_pick_and_records_both() {
    let frozen = answer(serde_json::json!({"op": "freeze", "frozen": null, "plan": fall(), "network_hash": hash(1), "purpose": "p"}))["frozen"].clone();
    let set = answer(serde_json::json!({"op": "adopt", "published": null, "frozen": frozen, "network_hash": hash(1), "today": "2026-01-01"}))["published"].clone();
    let got = answer(serde_json::json!({ "op": "receive", "documents": {"plans": set},
        "write": { "header": header(1, None) }, "ops": [{"cap": "plans", "op": "migrate"}] }));
    assert_eq!(entry_kinds(&got), vec!["frozen_version", "frozen_versions", "pick", "plans"], "{got}");
    let entry: serde_json::Value = serde_json::from_str(got["entry"].as_str().unwrap()).unwrap();
    let version = entry["changes"].as_array().unwrap().iter().find(|c| c["kind"] == "frozen_version").unwrap();
    assert_eq!(version["after"]["purpose"], "published before the plans log", "{version}");
}

// ---------------------------------------------------------------------
// pick: a pick's checks in the plans log's write (D-237)
// ---------------------------------------------------------------------

/// A kernel op on the module (a load), answered.
fn kernel(req: serde_json::Value) -> serde_json::Value {
    let out: serde_json::Value = serde_json::from_str(&cap_plans::invoke(&req.to_string())).unwrap();
    out.get("ok").cloned().unwrap_or_else(|| panic!("{req}: {out}"))
}

/// The sample network as a pick of `plan`, registered: its hash.
fn registered_pick(plan: serde_json::Value) -> String {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/public/caps/sample-network-load.json")).unwrap();
    let mut network = serde_json::from_str::<serde_json::Value>(&text).unwrap()["network"].clone();
    network["plan"] = plan;
    kernel(serde_json::json!({"op": "load_network", "network": network}))["network_hash"].as_str().unwrap().to_string()
}

fn fall_2026() -> serde_json::Value {
    serde_json::json!({"id": "fall-2026", "name": "Fall 2026", "effective_from": "2026-10-01"})
}

/// A stops document over the sample's stops: A renamed before the pick's
/// date, B retired inside its period.
fn registered_stops() -> String {
    let stops = serde_json::json!({"kind": "stops", "schema_version": 1, "stops": {
        "A": [{"on": "2020-01-01", "event": "in_service", "lat": 45.5, "lon": -73.5, "name": "Main & 1st"},
              {"on": "2026-09-01", "event": "changed", "name": "A Street Station"}],
        "B": [{"on": "2020-01-01", "event": "in_service", "lat": 45.51, "lon": -73.51, "name": "Main & 2nd"},
              {"on": "2026-11-02", "event": "retired"}]
    }});
    kernel(serde_json::json!({"op": "load_stops", "stops": stops}))["stops_hash"].as_str().unwrap().to_string()
}

fn pick_write(loaded: serde_json::Value, args: serde_json::Value) -> serde_json::Value {
    answer(serde_json::json!({ "op": "receive", "write": { "header": header(1, None) }, "loaded": loaded,
        "ops": [{"cap": "plans", "op": "pick", "args": args}] }))
}

/// A pick that does not publish is refused as cap-network's
/// `publishable` answers it, with no entry and nothing to store.
#[test]
fn a_pick_that_does_not_publish_is_refused_with_its_issues_and_refusals() {
    let network_hash = registered_pick(serde_json::json!({"id": "far", "name": "Far", "effective_from": "2199-01-01"}));
    let got = pick_write(serde_json::json!({"network_hash": network_hash}),
        serde_json::json!({"purpose": "the published pick", "adopt": true, "today": "2026-01-01"}));
    let refusals = &got["refused"]["refusals"];
    assert!(refusals["issues"].is_array(), "{got}");
    assert_eq!(refusals["refusals"][0].as_object().map(|o| o.keys().next().cloned()), Some(Some("no_service_in_plan_period".into())), "{got}");
    assert!(got.get("entry").is_none() && got.get("made").is_none(), "{got}");
}

/// A pick frozen and adopted in one entry names the network for the host
/// to store first: the canonical bytes the kernel registered, as the head
/// of plan.<id>, the hash the adoption names.
#[test]
fn a_pick_freezes_and_adopts_and_names_the_network_to_store_first() {
    let network_hash = registered_pick(fall_2026());
    let got = pick_write(serde_json::json!({"network_hash": network_hash}),
        serde_json::json!({"purpose": "the published pick", "adopt": true, "today": "2026-01-01", "stops_seq": 4}));
    assert_eq!(entry_kinds(&got), vec!["frozen_version", "frozen_versions", "pick", "plans"], "{got}");
    let made = &got["made"][0];
    assert_eq!(made["pointer"], "plan.fall-2026");
    assert_eq!(made["kind"], "network_document");
    assert_eq!(made["document_hash"], network_hash, "no stops were read: the registered bytes");
    assert_eq!(cap_abi::content_hash(made["document"].as_str().unwrap().as_bytes()), network_hash);
    assert_eq!(got["answer"]["plan_id"], "fall-2026");
    assert_eq!(got["answer"]["network_hash"], network_hash);
    assert_eq!(got["answer"]["stop_warnings"], serde_json::json!([]));
}

/// A pick brought forward by a migration names the version it came from
/// (D-232): its frozen version pins the source by its network hash.
#[test]
fn a_pick_brought_forward_pins_the_version_it_came_from() {
    let network_hash = registered_pick(fall_2026());
    let source = format!("sha256:{}", "ab".repeat(32));
    let got = pick_write(serde_json::json!({"network_hash": network_hash}),
        serde_json::json!({"purpose": "a published pick, brought forward to network v2", "from_network_hash": source, "adopt": true, "today": "2026-01-01"}));
    let entry: serde_json::Value = serde_json::from_str(got["entry"].as_str().unwrap_or_else(|| panic!("{got}"))).unwrap();
    let version = entry["changes"].as_array().unwrap().iter().find(|c| c["kind"] == "frozen_version").expect("a frozen version");
    assert_eq!(version["after"]["from_network_hash"], serde_json::json!(source), "{version}");
}

/// An adopted pick reads the stops document over its period (D-157): the
/// stored network carries each stop's record on its first date, and the
/// warnings name a stop not in service inside it. The same answer as
/// cap-stops' `read_pick`, which calls the same library function.
#[test]
fn an_adopted_pick_reads_the_stops_document() {
    let network_hash = registered_pick(fall_2026());
    let got = pick_write(serde_json::json!({"network_hash": network_hash, "stops_hash": registered_stops()}),
        serde_json::json!({"purpose": "the published pick", "adopt": true, "today": "2026-01-01"}));
    let made = &got["made"][0];
    let stored: serde_json::Value = serde_json::from_str(made["document"].as_str().unwrap()).unwrap();
    assert_eq!(stored["stops"]["A"]["name"], "A Street Station", "{got}");
    assert_ne!(made["document_hash"], network_hash);
    assert_eq!(got["answer"]["network_hash"], made["document_hash"], "the adoption names the stored bytes");
    assert_eq!(got["answer"]["stop_warnings"][0]["stop_not_in_service"]["stop"], "B", "{got}");
}

/// A freeze alone puts nothing in force and reads no stops; a pick with
/// no registered network does not run.
#[test]
fn a_freeze_alone_reads_no_stops_and_a_pick_needs_its_network() {
    let network_hash = registered_pick(fall_2026());
    let got = pick_write(serde_json::json!({"network_hash": network_hash, "stops_hash": registered_stops()}),
        serde_json::json!({"purpose": "proposal for public comment", "today": "2026-01-01"}));
    assert_eq!(entry_kinds(&got), vec!["frozen_version", "frozen_versions"], "{got}");
    assert_eq!(got["made"][0]["document_hash"], network_hash, "a freeze stores the network as sent");
    let none = pick_write(serde_json::json!({}), serde_json::json!({"adopt": true, "today": "2026-01-01"}));
    assert!(none["failed"]["error"].as_str().unwrap().contains("load_network"), "{none}");
}
