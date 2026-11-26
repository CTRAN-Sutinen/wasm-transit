//! C-TRAN's agreement with ATU Local 757 for 2022–2025 as work rules. It
//! is a test case for the model, never a default. Where a clause reads two
//! ways, each reading is its own document, built from the base one.
//!
//! The base document holds no rule for Art. 19 §8's "no more than 5% of
//! the runs shall pay less than two hours": the mix bounds the share of
//! classes, and a run's pay is not a class.

use serde_json::json;
use transit_runs::WorkRules;

fn rules(value: serde_json::Value) -> WorkRules {
    serde_json::from_value::<WorkRules>(value).expect("the fixture reads").checked().expect("the fixture passes")
}

fn base() -> serde_json::Value {
    json!({
        "kind": "work_rules",
        "schema_version": transit_runs::SCHEMA_VERSION,
        "name": "C-TRAN fixed route, ATU Local 757, 2022–2025",
        "source": {"title": "Labor agreement, C-TRAN and ATU Local 757", "effective_from": "2022-01-01", "effective_to": "2025-12-31"},
        "measures": {
            "driving": ["revenue", "deadhead"],
            "platform": ["revenue", "deadhead", "layover"],
            "worked": ["report", "revenue", "deadhead", "layover", "travel", "standby", "clear"],
            "paid": ["report", "revenue", "deadhead", "layover", "travel", "standby", "clear", "gap"]
        },
        "allowances": [
            {"event": "pull_out", "secs": 600, "activity": "report", "cites": "Art. 19 §3 (the employer sets the minutes; 600 s is a placeholder)"},
            {"event": "pull_in", "secs": 300, "activity": "clear", "cites": "Art. 19 §3 (the employer sets the minutes; 300 s is a placeholder)"}
        ],
        "gaps": [{"from_secs": 0, "paid_pct": 100, "cites": "Art. 19 §8, type A: no deductible period"}],
        "limits": [
            {"quantity": "measure", "measure": "driving", "max": 43_200, "strength": "waivable", "cites": "Art. 19 §3"},
            {"quantity": "spread", "max": 54_000, "strength": "waivable", "cites": "Art. 19 §3"},
            {"quantity": "days_worked", "max": 5, "over": "week", "cites": "Art. 19 §2: two days' rest in seven"},
            {"quantity": "rest", "min": 32_400, "over": "rolling24h", "cites": "Art. 19 §9"}
        ],
        "premiums": [
            {"on": "measure", "measure": "worked", "after_secs": 28_800, "pays": "worked", "pct": 150, "cites": "Art. 22 §1, Art. 31"}
        ],
        "guarantees": [{"per": "report", "min_secs": 7_200, "counts": "paid", "cites": "Art. 19 §6"}],
        "travel": [
            {"at": "day_end", "beyond_m": 2_414, "within_m": 402, "timed": "next_trip", "cites": "Art. 14 §4"}
        ],
        "combine": "highest",
        "classes": [
            {"id": "A", "guarantees": [{"per": "day", "min_secs": 28_800, "counts": "paid", "cites": "Art. 19 §2"}],
             "cites": "Art. 19 §8"},
            {"id": "B",
             "conditions": [{"quantity": "pieces", "min": 2}, {"quantity": "gap", "min": 1_800}],
             "gaps": [{"from_secs": 0, "paid_pct": 100}, {"from_secs": 1_800, "paid_pct": 0, "cites": "Art. 19 §8: a deductible period of at least 30 minutes"}],
             "premiums": [{"on": "spread", "after_secs": 37_800, "pays": "worked", "pct": 150, "cites": "Art. 19 §8"}],
             "guarantees": [{"per": "day", "min_secs": 28_800, "counts": "paid", "cites": "Art. 19 §2"}],
             "cites": "Art. 19 §8"},
            {"id": "C",
             "conditions": [{"quantity": "pieces", "min": 2}, {"quantity": "gap", "min": 1_800}],
             "gaps": [{"from_secs": 0, "paid_pct": 100}, {"from_secs": 1_800, "paid_pct": 0, "cites": "Art. 19 §8: a deductible period of at least 30 minutes"}],
             "premiums": [{"on": "spread", "after_secs": 48_600, "pays": "worked", "pct": 150, "cites": "Art. 19 §8"}],
             "guarantees": [{"per": "day", "min_secs": 28_800, "counts": "paid", "cites": "Art. 19 §2"}],
             "cites": "Art. 19 §8"},
            {"id": "part_time", "replaces": ["guarantees"],
             "guarantees": [{"per": "report", "min_secs": 5_400, "counts": "paid", "cites": "Art. 38"}],
             "cites": "Art. 45 (3)"}
        ],
        "mix": [
            {"classes": ["A"], "min_pct": 35, "target_pct": 45, "cites": "Art. 19 §8"},
            {"classes": ["A", "B"], "target_pct": 70, "cites": "Art. 19 §8"},
            {"classes": ["C"], "max_pct": 30, "cites": "Art. 19 §8"}
        ],
        "objective": {"maximize": ["A", "B", "C"], "minimize_premium": true}
    })
}

/// The agreement as written, with "drive" read as time driving.
pub fn ctran_2022_work_rules() -> WorkRules {
    rules(base())
}

/// "Drive 12 hours" read as platform time, layovers included (Art. 19 §3).
pub fn ctran_2022_drive_as_platform() -> WorkRules {
    let mut doc = base();
    doc["limits"][0]["measure"] = json!("platform");
    rules(doc)
}

/// Washington's rest and meal breaks added (WAC 296-126-092), as an
/// approximation: a paid rest in each four hours, and an unpaid meal
/// between the second and fifth hour of a run over five hours.
pub fn ctran_2022_with_state_breaks() -> WorkRules {
    let mut doc = base();
    doc["breaks"] = json!([
        {"min_secs": 1_800, "window_from_secs": 7_200, "window_to_secs": 18_000, "when_measure": "worked", "when_over_secs": 18_000, "cites": "WAC 296-126-092 (1)"},
        {"min_secs": 600, "window_from_secs": 0, "window_to_secs": 14_400, "paid": true, "when_measure": "worked", "when_over_secs": 7_200, "cites": "WAC 296-126-092 (4)"},
        {"min_secs": 600, "window_from_secs": 14_400, "window_to_secs": 28_800, "paid": true, "when_measure": "worked", "when_over_secs": 21_600, "cites": "WAC 296-126-092 (4)"}
    ]);
    rules(doc)
}

/// Travel paid between the pieces of a run too, not only at the day's end
/// (Art. 14 §4).
pub fn ctran_2022_travel_between_pieces() -> WorkRules {
    let mut doc = base();
    doc["travel"].as_array_mut().expect("travel is a list").push(json!(
        {"at": "between_pieces", "beyond_m": 2_414, "within_m": 402, "timed": "next_trip", "cites": "Art. 14 §4, read to cover pieces"}
    ));
    rules(doc)
}
