//! Freezing and adopting (D-191). A frozen version is in the record and not
//! in force. Only a frozen version is adopted.

use proptest::prelude::*;
use transit_date::Date;
use transit_plans::{
    adopt, freeze, freeze_from, in_force, publish, retire, FrozenVersions, PlanInfo, PlanIssue,
    PublishedPlans, FROZEN_SCHEMA_VERSION,
};

fn info(id: &str, from: &str) -> PlanInfo {
    PlanInfo {
        id: id.into(),
        name: format!("{id} pick"),
        effective_from: Some(from.into()),
    }
}

fn hash(n: u8) -> String {
    format!("sha256:{}", format!("{n:02x}").repeat(32))
}

fn date(s: &str) -> Date {
    Date::parse(s).unwrap()
}

fn early() -> Date {
    date("2000-01-01")
}

#[test]
fn the_empty_frozen_versions_carry_their_version() {
    let frozen = FrozenVersions::empty();
    assert_eq!(frozen.schema_version, FROZEN_SCHEMA_VERSION);
    assert!(frozen.versions.is_empty());
}

#[test]
fn a_freeze_records_the_network_by_its_hash_with_what_it_is() {
    let before = FrozenVersions::empty();
    let frozen = freeze(
        &before,
        &info("fall", "2026-09-07"),
        &hash(1),
        "proposal for public comment",
    )
    .unwrap();
    let version = frozen
        .versions
        .get(&hash(1))
        .expect("keyed by the network's hash");
    assert_eq!(version.network_hash, hash(1));
    assert_eq!(
        (version.plan_id.as_str(), version.effective_from.as_str()),
        ("fall", "2026-09-07")
    );
    assert_eq!(version.purpose, "proposal for public comment");
    assert!(before.versions.is_empty(), "the input is never mutated");
}

#[test]
fn a_freeze_says_what_it_is_and_is_a_dated_pick() {
    let none = FrozenVersions::empty();
    assert_eq!(
        freeze(&none, &info("fall", "2026-09-07"), &hash(1), "  "),
        Err(vec![PlanIssue::FreezeWithoutPurpose])
    );
    assert_eq!(
        freeze(
            &none,
            &PlanInfo {
                id: String::new(),
                name: "import".into(),
                effective_from: None
            },
            &hash(1),
            "proposal"
        ),
        Err(vec![PlanIssue::UnversionedPlan])
    );
    assert_eq!(
        freeze(
            &none,
            &PlanInfo {
                id: "fall".into(),
                name: "fall".into(),
                effective_from: None
            },
            &hash(1),
            "proposal"
        ),
        Err(vec![PlanIssue::PlanMissingEffectiveDate {
            id: "fall".into()
        }])
    );
}

#[test]
fn each_version_of_a_plan_is_kept_and_a_refreeze_says_what_it_is_now() {
    let frozen = freeze(
        &FrozenVersions::empty(),
        &info("fall", "2026-09-07"),
        &hash(1),
        "proposal for public comment",
    )
    .unwrap();
    let frozen = freeze(
        &frozen,
        &info("fall", "2026-09-07"),
        &hash(2),
        "board packet",
    )
    .unwrap();
    assert_eq!(
        frozen.versions.len(),
        2,
        "two networks of one plan are two versions"
    );
    let again = freeze(
        &frozen,
        &info("fall", "2026-09-07"),
        &hash(1),
        "final proposal",
    )
    .unwrap();
    assert_eq!(again.versions.len(), 2, "one record for each network");
    assert_eq!(again.versions[&hash(1)].purpose, "final proposal");
}

#[test]
fn a_frozen_version_is_not_in_force() {
    let set = PublishedPlans::empty();
    freeze(
        &FrozenVersions::empty(),
        &info("fall", "2026-09-07"),
        &hash(1),
        "proposal",
    )
    .unwrap();
    assert_eq!(
        in_force(&set, date("2026-10-01")).unwrap(),
        None,
        "a freeze changes no plan set"
    );
}

#[test]
fn only_a_frozen_version_is_adopted() {
    let frozen = freeze(
        &FrozenVersions::empty(),
        &info("fall", "2026-09-07"),
        &hash(1),
        "proposal",
    )
    .unwrap();
    assert_eq!(
        adopt(&PublishedPlans::empty(), &frozen, &hash(2), early(), None),
        Err(vec![PlanIssue::NotFrozen {
            network_hash: hash(2)
        }])
    );
}

#[test]
fn adopting_is_publishing_the_frozen_version_under_its_own_frame() {
    let frozen = freeze(
        &FrozenVersions::empty(),
        &info("fall", "2026-09-07"),
        &hash(1),
        "proposal",
    )
    .unwrap();
    let adopted = adopt(&PublishedPlans::empty(), &frozen, &hash(1), early(), None).unwrap();
    let published = publish(
        &PublishedPlans::empty(),
        &info("fall", "2026-09-07"),
        &hash(1),
        early(),
    )
    .unwrap();
    assert_eq!(adopted, published);
    let pick = in_force(&adopted, date("2026-10-01")).unwrap().unwrap();
    assert_eq!(
        (pick.id.as_str(), pick.network_hash.as_str()),
        ("fall", hash(1).as_str())
    );
}

#[test]
fn an_adoption_names_the_stops_log_entry_it_read() {
    let frozen = freeze(
        &FrozenVersions::empty(),
        &info("fall", "2026-09-07"),
        &hash(1),
        "proposal",
    )
    .unwrap();
    let adopted = adopt(
        &PublishedPlans::empty(),
        &frozen,
        &hash(1),
        early(),
        Some(42),
    )
    .unwrap();
    assert_eq!(adopted.plans[0].stops_seq, Some(42));
    let json = serde_json::to_string(
        &adopt(&PublishedPlans::empty(), &frozen, &hash(1), early(), None).unwrap(),
    )
    .unwrap();
    assert!(
        !json.contains("stops_seq"),
        "a set that read no stops keeps its bytes: {json}"
    );
}

#[test]
fn an_adoption_keeps_the_chain_rules() {
    let frozen = freeze(
        &FrozenVersions::empty(),
        &info("fall", "2026-09-07"),
        &hash(1),
        "proposal",
    )
    .unwrap();
    let today = date("2026-10-01");
    assert!(matches!(
        adopt(&PublishedPlans::empty(), &frozen, &hash(1), today, None),
        Err(issues) if matches!(issues[..], [PlanIssue::PickDatedInPast { .. }])
    ));
}

#[derive(Debug, Clone)]
enum Step {
    Freeze(u8, u8, u8),
    Adopt(u8),
    Retire(u8),
}

fn arb_step() -> impl Strategy<Value = Step> {
    prop_oneof![
        (0u8..3, 1u8..8, 0u8..20).prop_map(|(p, n, d)| Step::Freeze(p, n, d)),
        (1u8..8).prop_map(Step::Adopt),
        (0u8..3).prop_map(Step::Retire),
    ]
}

proptest! {
    #[test]
    fn every_network_in_force_was_frozen(steps in proptest::collection::vec(arb_step(), 0..30)) {
        let plans = ["spring", "fall", "winter"];
        let mut set = PublishedPlans::empty();
        let mut frozen = FrozenVersions::empty();
        for step in steps {
            match step {
                Step::Freeze(p, n, d) => {
                    let from = format!("2026-{:02}-{:02}", 1 + d % 12, 1 + d % 28);
                    if let Ok(next) = freeze(&frozen, &info(plans[p as usize], &from), &hash(n), "proposal") {
                        frozen = next;
                    }
                }
                Step::Adopt(n) => {
                    if let Ok(next) = adopt(&set, &frozen, &hash(n), early(), None) {
                        set = next;
                    }
                }
                Step::Retire(p) => {
                    if let Ok(next) = retire(&set, plans[p as usize], early()) {
                        set = next;
                    }
                }
            }
            for pick in &set.plans {
                prop_assert!(frozen.versions.contains_key(&pick.network_hash), "{} names {}, which is not frozen", pick.id, pick.network_hash);
            }
        }
    }
}

/// A version that a migration brought forward pins its source by network
/// hash (D-232). A later freeze that names no source keeps it.
#[test]
fn a_version_brought_forward_pins_the_version_it_came_from() {
    let fall = info("fall", "2026-09-07");
    let frozen = freeze(&FrozenVersions::empty(), &fall, &hash(1), "board packet").unwrap();
    assert_eq!(
        frozen.versions[&hash(1)].from_network_hash,
        None,
        "written as it is"
    );
    let forward = freeze_from(
        &frozen,
        &fall,
        &hash(2),
        "a published pick, brought forward to network v2",
        Some(&hash(1)),
    )
    .unwrap();
    assert_eq!(
        forward.versions[&hash(2)].from_network_hash.as_deref(),
        Some(hash(1).as_str())
    );
    assert_eq!(
        forward.versions[&hash(1)],
        frozen.versions[&hash(1)],
        "the source version stays as it was"
    );
    let again = freeze(&forward, &fall, &hash(2), "adopted").unwrap();
    assert_eq!(
        again.versions[&hash(2)].from_network_hash.as_deref(),
        Some(hash(1).as_str()),
        "a refreeze keeps its source"
    );
}
