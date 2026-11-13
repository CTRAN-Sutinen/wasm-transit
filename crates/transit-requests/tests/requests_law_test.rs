//! The stop requests document's laws (D-159), derived from the user's
//! intent — C-TRAN's Bus Stop Sign & Shelter Installation Worksheet,
//! which "the planning team submits, the sheet isn't itself a work
//! order. they should have their own record and the system should be
//! able to track a reference to the external work order" — and written
//! before the implementation:
//!
//! * a request is a record of its own, by id: the job (the worksheet's
//!   five), the stop it concerns, its status (drafted, submitted, work
//!   order opened, completed, cancelled — cancelled is the tombstone:
//!   nothing is deleted), and the worksheet's fields, every one
//!   optional but the stop and the job;
//! * a write refuses only what makes a request wrong, each reason named:
//!   no stop, a job or status or listed answer this build does not
//!   know, a date that is not a date, a negative offset, a request at
//!   "work order opened" with no work order id, a completed one with no
//!   completion date, or one completed before it was requested;
//! * `put` places one request by id (new, or replacing the one held)
//!   and nothing else changes; `for_stop` is a stop's requests in id
//!   order;
//! * one document is one byte sequence however its keys are ordered,
//!   and what this repo does not know — another tool's field — is kept.

use proptest::prelude::*;
use serde_json::{json, Value};
use transit_requests::{RequestProblem, StopRequest, StopRequests, JOBS, STATUSES};

fn request(stop: &str, job: &str, status: &str) -> StopRequest {
    serde_json::from_value(json!({ "stop": stop, "job": job, "status": status })).unwrap()
}

fn doc(requests: Vec<(&str, StopRequest)>) -> StopRequests {
    StopRequests { requests: requests.into_iter().map(|(k, v)| (k.to_string(), v)).collect(), ..StopRequests::default() }
}

fn refused(d: StopRequests) -> Vec<RequestProblem> {
    d.checked().err().unwrap_or_default().into_iter().map(|r| r.problem).collect()
}

fn refused_json(v: Value) -> Vec<RequestProblem> {
    refused(serde_json::from_value(v).unwrap())
}

#[test]
fn every_job_and_status_the_worksheet_names_is_accepted() {
    assert_eq!(JOBS, ["new", "reinstall", "relocate_remove", "repair_improve", "temporary"]);
    assert_eq!(STATUSES, ["drafted", "submitted", "ordered", "completed", "cancelled"]);
    for job in JOBS {
        assert_eq!(refused(doc(vec![("r1", request("6124", job, "drafted"))])), vec![], "{job}");
    }
}

#[test]
fn the_worked_request_reads() {
    let d: StopRequests = serde_json::from_value(json!({
        "kind": "stop_requests", "schema_version": 1,
        "requests": {
            "R-2027-014": {
                "stop": "N1", "job": "new", "status": "ordered",
                "requested_by": "Planning", "requested_on": "2027-01-04",
                "routes": ["37"], "direction": "eastbound",
                "street": "Mill Plain Blvd", "cross_street": "NE 112th Ave", "offset_ft": 60, "side": "south",
                "city": "Vancouver", "county": "Clark", "township": "2N", "range": "2E", "section": "15", "parcel": "1234-56-789",
                "marked_by": "curb_mark",
                "locate": { "ticket": "26123456", "called_on": "2027-01-06", "called_by": "Planning", "utilities": ["City of Vancouver", "NW Natural"] },
                "work_order": { "system": "Maintenance", "id": "WO-88123", "url": "https://wo.example/88123", "status": "open", "read_on": "2027-01-07" }
            }
        }
    }))
    .unwrap();
    assert_eq!(refused(d.clone()), vec![]);
    assert_eq!(d.for_stop("N1").len(), 1);
}

#[test]
fn a_request_names_its_stop_and_a_known_job_and_status() {
    assert_eq!(refused(doc(vec![("r", request("", "new", "drafted"))])), vec![RequestProblem::NoStop]);
    assert_eq!(refused(doc(vec![("r", request("a", "paint", "drafted"))])), vec![RequestProblem::Unknown { field: "job".into(), value: "paint".into() }]);
    assert_eq!(refused(doc(vec![("r", request("a", "new", "lost"))])), vec![RequestProblem::Unknown { field: "status".into(), value: "lost".into() }]);
    assert_eq!(refused(doc(vec![("", request("a", "new", "drafted"))])), vec![RequestProblem::NoId]);
}

#[test]
fn a_listed_answer_is_one_the_worksheet_lists() {
    let with = |field: &str, value: &str| {
        let mut r = serde_json::to_value(request("a", "new", "drafted")).unwrap();
        r[field] = json!(value);
        refused_json(json!({ "requests": { "r": r } }))
    };
    for (field, good) in [("side", "north"), ("direction", "clockwise"), ("marked_by", "stake")] {
        assert_eq!(with(field, good), vec![], "{field}");
        assert_eq!(with(field, "sideways"), vec![RequestProblem::Unknown { field: field.into(), value: "sideways".into() }], "{field}");
    }
}

#[test]
fn dates_are_dates_and_an_offset_is_not_negative() {
    let r = json!({ "stop": "a", "job": "new", "status": "drafted", "requested_on": "Jan 4" });
    assert_eq!(refused_json(json!({ "requests": { "r": r } })), vec![RequestProblem::BadDate { date: "Jan 4".into() }]);
    let r = json!({ "stop": "a", "job": "new", "status": "drafted", "locate": { "called_on": "2027-13-01" } });
    assert_eq!(refused_json(json!({ "requests": { "r": r } })), vec![RequestProblem::BadDate { date: "2027-13-01".into() }]);
    let r = json!({ "stop": "a", "job": "new", "status": "drafted", "offset_ft": -3 });
    assert_eq!(refused_json(json!({ "requests": { "r": r } })), vec![RequestProblem::NegativeOffset]);
}

#[test]
fn a_status_carries_what_it_claims() {
    let r = json!({ "stop": "a", "job": "new", "status": "ordered" });
    assert_eq!(refused_json(json!({ "requests": { "r": r } })), vec![RequestProblem::NoWorkOrder]);
    let r = json!({ "stop": "a", "job": "new", "status": "ordered", "work_order": { "system": "Maintenance", "id": "" } });
    assert_eq!(refused_json(json!({ "requests": { "r": r } })), vec![RequestProblem::NoWorkOrder]);
    let r = json!({ "stop": "a", "job": "new", "status": "completed", "work_order": { "id": "WO-1" } });
    assert_eq!(refused_json(json!({ "requests": { "r": r } })), vec![RequestProblem::NoCompletion]);
    let r = json!({ "stop": "a", "job": "new", "status": "completed", "requested_on": "2027-02-01", "completed_on": "2027-01-15" });
    assert_eq!(refused_json(json!({ "requests": { "r": r } })), vec![RequestProblem::CompletedBeforeRequested]);
    // a cancelled request keeps whatever it had
    let r = json!({ "stop": "a", "job": "new", "status": "cancelled" });
    assert_eq!(refused_json(json!({ "requests": { "r": r } })), vec![]);
}

#[test]
fn every_refusal_names_its_request() {
    let refusals = doc(vec![("good", request("a", "new", "drafted")), ("bad", request("", "new", "drafted"))]).checked().unwrap_err();
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0].request, "bad");
}

fn arb_request() -> impl Strategy<Value = StopRequest> {
    (
        "[A-Z0-9]{1,5}",
        proptest::sample::select(JOBS.to_vec()),
        proptest::sample::select(vec!["drafted", "submitted", "cancelled"]),
        proptest::option::of("[a-z ]{0,10}"),
    )
        .prop_map(|(stop, job, status, note)| {
            let mut r = request(&stop, job, status);
            r.note = note;
            r
        })
}

fn arb_doc() -> impl Strategy<Value = StopRequests> {
    proptest::collection::btree_map("[a-z0-9-]{1,6}", arb_request(), 0..6).prop_map(|requests| StopRequests { requests, ..StopRequests::default() })
}

proptest! {
    /// put places one request by id and nothing else changes.
    #[test]
    fn put_places_one_request(d in arb_doc(), id in "[a-z0-9-]{1,6}", r in arb_request()) {
        let next = d.put(&id, r.clone());
        prop_assert_eq!(next.requests.get(&id), Some(&r));
        for (k, v) in &d.requests {
            if *k != id {
                prop_assert_eq!(next.requests.get(k), Some(v));
            }
        }
        prop_assert_eq!(next.requests.len(), d.requests.len() + usize::from(!d.requests.contains_key(&id)));
    }

    /// A stop's requests are exactly those naming it, in id order.
    #[test]
    fn for_stop_is_the_stops_requests(d in arb_doc(), pick in any::<prop::sample::Index>()) {
        let stops: Vec<String> = d.requests.values().map(|r| r.stop.clone()).collect();
        let stop = if stops.is_empty() { "none".to_string() } else { stops[pick.index(stops.len())].clone() };
        let got: Vec<String> = d.for_stop(&stop).into_iter().map(|(id, _)| id.to_string()).collect();
        let want: Vec<String> = d.requests.iter().filter(|(_, r)| r.stop == stop).map(|(k, _)| k.clone()).collect();
        prop_assert_eq!(got, want);
    }

    /// What this repo does not know survives a read and a write, and the
    /// bytes do not depend on the order a writer gave the keys.
    #[test]
    fn unknown_fields_survive_and_bytes_are_canonical(d in arb_doc(), name in "x_[a-z]{1,8}", value in "[a-z ]{0,12}") {
        let mut v = serde_json::to_value(&d).unwrap();
        v[name.as_str()] = json!(value);
        if let Some(first) = v["requests"].as_object_mut().unwrap().values_mut().next() {
            first[name.as_str()] = json!({ "nested": [value.clone(), 1] });
        }
        let read: StopRequests = serde_json::from_value(v.clone()).unwrap();
        prop_assert_eq!(serde_json::to_value(&read).unwrap(), v.clone());
        // the same document with its keys reversed is the same bytes
        fn reverse(v: &Value) -> Value {
            match v {
                Value::Object(m) => {
                    let mut out = serde_json::Map::new();
                    for (k, x) in m.iter().rev() { out.insert(k.clone(), reverse(x)); }
                    Value::Object(out)
                }
                Value::Array(a) => Value::Array(a.iter().map(reverse).collect()),
                x => x.clone(),
            }
        }
        let again: StopRequests = serde_json::from_str(&reverse(&v).to_string()).unwrap();
        prop_assert_eq!(serde_json::to_string(&again).unwrap(), serde_json::to_string(&read).unwrap());
    }
}

/// What the work order system answers lands on the request (D-165): its
/// reference replaces the one held (a link or status the answer lacks is
/// kept), a drafted or submitted request moves to `ordered` — its work
/// order is open — and a request already ordered, completed, or cancelled
/// keeps its status; every other request is as it was, and an id the
/// document does not hold answers none.
#[test]
fn a_work_order_lands_on_its_request() {
    let held = doc(vec![
        ("a", request("1", "new", "submitted")),
        ("b", StopRequest { completed_on: Some("2027-01-02".into()), ..request("2", "new", "completed") }),
        ("c", request("3", "new", "drafted")),
    ]);
    let wo = |id: &str, status: &str| transit_requests::WorkOrder { system: Some("Maintenance".into()), id: id.into(), status: Some(status.into()), read_on: Some("2027-01-07".into()), ..Default::default() };
    let next = held.record_work_order("a", wo("WO-1", "open")).unwrap();
    assert_eq!(next.requests["a"].status, "ordered");
    assert_eq!(next.requests["a"].work_order.as_ref().unwrap().id, "WO-1");
    assert_eq!(next.requests["b"], held.requests["b"]);
    assert_eq!(next.clone().checked().map(|_| ()), Ok(()));
    // a later status: the link it lacks is kept, the status stays ordered
    let mut linked = next.clone();
    linked.requests.get_mut("a").unwrap().work_order.as_mut().unwrap().url = Some("https://wo/1".into());
    let later = linked.record_work_order("a", transit_requests::WorkOrder { url: None, ..wo("WO-1", "in progress") }).unwrap();
    let w = later.requests["a"].work_order.as_ref().unwrap();
    assert_eq!((w.status.as_deref(), w.url.as_deref()), (Some("in progress"), Some("https://wo/1")));
    assert_eq!(later.requests["a"].status, "ordered");
    // completed stays completed
    assert_eq!(held.record_work_order("b", wo("WO-2", "closed")).unwrap().requests["b"].status, "completed");
    assert_eq!(held.record_work_order("c", wo("WO-3", "open")).unwrap().requests["c"].status, "ordered");
    assert!(held.record_work_order("zz", wo("WO-9", "open")).is_none());
}

/// A request carries its photos and the site drawing (D-162), each named
/// by its content hash with its media type; another shape is refused.
#[test]
fn a_request_names_its_attachments_by_hash() {
    let hash = format!("sha256:{}", "cd".repeat(32));
    let r = json!({ "stop": "a", "job": "new", "status": "drafted", "attachments": [{ "hash": hash, "type": "application/pdf", "name": "site drawing.pdf" }] });
    assert_eq!(refused_json(json!({ "requests": { "r": r } })), vec![]);
    let r = json!({ "stop": "a", "job": "new", "status": "drafted", "attachments": [{ "hash": "sha256:xyz", "type": "image/jpeg" }] });
    assert_eq!(refused_json(json!({ "requests": { "r": r } })), vec![RequestProblem::BadAttachment { hash: "sha256:xyz".into() }]);
}
