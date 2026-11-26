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
