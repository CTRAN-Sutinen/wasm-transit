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
