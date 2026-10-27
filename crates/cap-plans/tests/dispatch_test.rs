//! Native dispatch tests for the plan-chain capability — the seam
//! without wasm, over plan-set metadata. The chain's own laws
//! (resolution, windows, validity, order-insensitivity) are tested
//! at the library seam (transit-plans); here the module's contract
//! is pinned: the tagged wire form round-trips, and garbage or
//! bulk bytes are honest errors, never panics.

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
