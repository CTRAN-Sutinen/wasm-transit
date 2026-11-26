//! The C-TRAN 2022–2025 work rules and their paired readings
//! (docs/units/runs-layer.md, "The labor rules model"). Each reading loads
//! through the kernel's parse, and differs from the base document only in
//! the rule that its reading changes.

use fixtures::*;
use serde_json::Value;
use transit_runs::{Per, WorkRules};
use transit_version::Load;

fn value(w: &WorkRules) -> Value {
    serde_json::to_value(w).unwrap()
}

fn without(mut v: Value, field: &str) -> Value {
    v.as_object_mut().unwrap().remove(field);
    v
}

#[test]
fn each_reading_loads_as_the_kernel_reads_it() {
    for w in [ctran_2022_work_rules(), ctran_2022_drive_as_platform(), ctran_2022_with_state_breaks(), ctran_2022_travel_between_pieces()] {
        let bytes = serde_json::to_vec(&w).unwrap();
        let loaded = WorkRules::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e:?}", w.name));
        assert_eq!(loaded, w);
    }
}

#[test]
fn each_reading_differs_only_where_it_reads_the_clause_another_way() {
    let base = value(&ctran_2022_work_rules());
    let platform = value(&ctran_2022_drive_as_platform());
    assert_eq!(without(platform.clone(), "limits"), without(base.clone(), "limits"));
    let measures = |v: &Value| v["limits"].as_array().unwrap().iter().filter_map(|l| l["measure"].as_str().map(String::from)).collect::<Vec<_>>();
    assert_eq!((measures(&base), measures(&platform)), (vec!["driving".to_string()], vec!["platform".to_string()]));

    let breaks = value(&ctran_2022_with_state_breaks());
    assert_eq!(without(breaks.clone(), "breaks"), base);
    assert_eq!(breaks["breaks"].as_array().unwrap().len(), 3);

    let travel = value(&ctran_2022_travel_between_pieces());
    assert_eq!(without(travel.clone(), "travel"), without(base.clone(), "travel"));
    assert_eq!((base["travel"].as_array().unwrap().len(), travel["travel"].as_array().unwrap().len()), (1, 2));
}

#[test]
fn the_classes_put_the_agreements_rules_in_force() {
    let w = ctran_2022_work_rules();
    let none = w.rules_for(None);
    assert_eq!(none.gap_paid_pct(3_600), 100, "a run in no class pays every gap");
    for class in ["B", "C"] {
        let r = w.rules_for(Some(class));
        assert_eq!((r.gap_paid_pct(1_799), r.gap_paid_pct(1_800)), (100, 0), "{class}: a gap of 30 minutes or more is unpaid");
        assert_eq!(r.premiums.len(), 2, "{class}: overtime and its spread premium");
    }
    let part_time = w.rules_for(Some("part_time"));
    assert_eq!(part_time.guarantees.iter().map(|g| (g.per, g.min_secs)).collect::<Vec<_>>(), vec![(Per::Report, 5_400)]);
    let a = w.rules_for(Some("A"));
    assert_eq!(a.guarantees.iter().map(|g| (g.per, g.min_secs)).collect::<Vec<_>>(), vec![(Per::Report, 7_200), (Per::Day, 28_800)]);
}

struct Line;

/// Stops sit on a line, a kilometer apart: "S3" is 3 km from "S0". The trip
/// home always leaves 15 minutes after the operator arrives.
impl transit_runs::Places for Line {
    fn meters(&self, a: &str, b: &str) -> Option<f64> {
        let n = |s: &str| s.trim_start_matches('S').parse::<i32>().ok();
        Some((n(a)? - n(b)?).abs() as f64 * 1_000.0)
    }

    fn next_trip_home(&self, _from: &str, after_secs: u32, _home: &str, _within_m: u32) -> Option<u32> {
        Some(after_secs + 900)
    }
}

const H: u32 = 3_600;

fn trip(name: &str, start: u32, end: u32, from: &str, to: &str) -> transit_runs::PieceTrip {
    transit_runs::PieceTrip { trip: name.into(), start_secs: start, end_secs: end, from: from.into(), to: to.into() }
}

fn piece(block: &str, trips: Vec<transit_runs::PieceTrip>) -> transit_runs::Piece {
    transit_runs::Piece { block: block.into(), trips, pull_out: true, pull_in: true }
}

fn names(j: &transit_runs::Judged) -> Vec<String> {
    j.broken.iter().map(|f| f.rule.clone()).collect()
}

/// 11 hours of driving with two hours of layover: 13 hours of platform.
#[test]
fn reading_drive_as_platform_limits_a_run_that_driving_time_does_not() {
    let p = vec![piece("B1", vec![trip("a", 5 * H, 10 * H + H / 2, "S0", "S0"), trip("b", 12 * H + H / 2, 18 * H, "S0", "S0")])];
    let driving = transit_runs::judge(&ctran_2022_work_rules(), Some("A"), &p, transit_runs::Day::Mon, &Line);
    let platform = transit_runs::judge(&ctran_2022_drive_as_platform(), Some("A"), &p, transit_runs::Day::Mon, &Line);
    assert!(!names(&driving).iter().any(|r| r.contains("driving")), "{:?}", names(&driving));
    assert!(names(&platform).iter().any(|r| r.contains("platform time")), "{:?}", names(&platform));
    assert!(platform.legal(), "the limit is waivable (Art. 19 §3)");
}

/// Nine hours straight, with no time off.
#[test]
fn reading_state_breaks_asks_a_long_run_for_a_meal() {
    let p = vec![piece("B1", vec![trip("a", 6 * H, 15 * H, "S0", "S0")])];
    let base = transit_runs::judge(&ctran_2022_work_rules(), Some("A"), &p, transit_runs::Day::Mon, &Line);
    let state = transit_runs::judge(&ctran_2022_with_state_breaks(), Some("A"), &p, transit_runs::Day::Mon, &Line);
    assert!(base.legal(), "{:?}", names(&base));
    assert!(names(&state).iter().any(|r| r.contains("1800 s")), "a missed meal: {:?}", names(&state));
    assert!(!state.legal());
}

/// The first piece ends 3 km from where the second begins. Ten hours of
/// work, so the pay is past the eight-hour guarantee.
#[test]
fn reading_travel_between_pieces_pays_the_ride_to_the_second_piece() {
    let p = vec![
        piece("B1", vec![trip("a", 5 * H, 10 * H, "S0", "S3")]),
        piece("B2", vec![trip("b", 13 * H, 18 * H, "S0", "S0")]),
    ];
    let base = transit_runs::judge(&ctran_2022_work_rules(), Some("B"), &p, transit_runs::Day::Mon, &Line);
    let travel = transit_runs::judge(&ctran_2022_travel_between_pieces(), Some("B"), &p, transit_runs::Day::Mon, &Line);
    assert_eq!(travel.measures["worked"], base.measures["worked"] + 900, "15 minutes of travel worked");
    assert!(travel.pay_secs > base.pay_secs);
}

/// A split run's 30-minute-plus gap is unpaid in class B, and paid in A.
#[test]
fn a_class_b_gap_is_deducted_and_a_class_a_gap_is_paid() {
    let p = vec![piece("B1", vec![trip("a", 6 * H, 10 * H, "S0", "S0")]), piece("B2", vec![trip("b", 14 * H, 18 * H, "S0", "S0")])];
    let a = transit_runs::judge(&ctran_2022_work_rules(), Some("A"), &p, transit_runs::Day::Mon, &Line);
    let b = transit_runs::judge(&ctran_2022_work_rules(), Some("B"), &p, transit_runs::Day::Mon, &Line);
    assert!(transit_runs::meets(&ctran_2022_work_rules(), "B", &p));
    assert!(a.paid_secs > b.paid_secs + 3 * H, "A pays the four-hour gap, B does not: {} vs {}", a.paid_secs, b.paid_secs);
    assert!(b.pay_secs >= 8 * H, "B's eight-hour day is guaranteed");
}
