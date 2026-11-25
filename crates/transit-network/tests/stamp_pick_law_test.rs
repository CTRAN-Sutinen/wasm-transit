//! An imported feed becomes a pick (D-140, D-228). A document that already
//! carries a plan id, or runs no service, gets no stamp.

use transit_network::{apply_edits, stamp_as_pick, first_service_date, Network, NetworkEdit};

fn network() -> Network {
    serde_json::from_str(include_str!("../../../testdata/network.json")).unwrap()
}

#[test]
fn a_feed_is_stamped_as_a_pick_on_its_first_service_date() {
    let mut feed = network();
    feed.plan = Default::default();
    let first = first_service_date(&feed).expect("the fixture runs service");
    let edits = stamp_as_pick(&feed, "Fall feed", "pick-1");
    let [NetworkEdit::SetPlan { plan, through }] = edits.as_slice() else { panic!("one set_plan edit: {edits:?}") };
    assert_eq!((plan.id.as_str(), plan.name.as_str(), plan.effective_from, through.as_deref()), ("pick-1", "Fall feed", Some(first), None));
    let stamped = apply_edits(&feed, &edits).unwrap();
    assert_eq!(stamped.plan.id, "pick-1");
}

#[test]
fn a_pick_and_a_feed_with_no_service_are_not_stamped() {
    let mut pick = network();
    pick.plan.id = "fall".into();
    assert!(stamp_as_pick(&pick, "x", "y").is_empty());
    let mut idle = network();
    idle.plan = Default::default();
    idle.pattern_trips.clear();
    assert!(first_service_date(&idle).is_none());
    assert!(stamp_as_pick(&idle, "x", "y").is_empty());
}
