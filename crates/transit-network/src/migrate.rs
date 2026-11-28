//! The body of the v1 to v2 step (`version.rs`). v1 is every untagged
//! document.
//!
//! A pre-alignment document draws the line of each pattern inline, as its
//! `path`. That line becomes an alignment under the id of the pattern, which
//! is the id that the export gave its shape.
//!
//! A pre-trip document keeps `departures` for each pattern trips and
//! references block trips as `(pattern_trips, start_secs)`. Every departure
//! becomes a one-start trip with the id `"{pattern_trips}@{start}"`, which
//! the export used. The profile that covered its start times it: timepoints
//! at the offsets of the profile, and other stops unscheduled, so they
//! materialize where they did.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::model::{Pattern, StopOffset};

pub(crate) fn migrate_v1(doc: &mut Value) -> Result<(), String> {
    let Some(root) = doc.as_object_mut() else {
        return Ok(());
    };
    migrate_paths(root);
    if let Some(old) = root.remove("schedules") {
        root.entry("pattern_trips").or_insert(old);
    }
    let pre_trip = root
        .get("pattern_trips")
        .and_then(Value::as_object)
        .is_some_and(|pts| pts.values().any(|pt| pt.get("trips").is_none()));
    if !pre_trip {
        return Ok(());
    }
    let patterns: BTreeMap<String, Pattern> = match root.get("patterns") {
        Some(p) => serde_json::from_value(p.clone()).map_err(|e| e.to_string())?,
        None => BTreeMap::new(),
    };
    if let Some(pts) = root.get_mut("pattern_trips").and_then(Value::as_object_mut) {
        for (key, pt) in pts.iter_mut() {
            let Some(pt) = pt.as_object_mut() else {
                continue;
            };
            if pt.contains_key("trips") {
                continue;
            }
            let departures = pt
                .remove("departures")
                .or_else(|| pt.remove("schedule"))
                .unwrap_or(Value::Null);
            let pattern = pt
                .get("pattern")
                .and_then(Value::as_str)
                .and_then(|p| patterns.get(p));
            let trips: Vec<Value> = old_starts(&departures)
                .into_iter()
                .map(|start| {
                    json!({
                        "id": format!("{key}@{start}"),
                        "departs": {"kind": "at", "start_secs": start},
                        "times": pattern.map(|p| profile_times(p, start)).unwrap_or_default(),
                    })
                })
                .collect();
            pt.insert("trips".into(), Value::Array(trips));
        }
    }
    if let Some(blocks) = root.get_mut("blocks").and_then(Value::as_object_mut) {
        for block in blocks.values_mut() {
            let Some(refs) = block.get_mut("trips").and_then(Value::as_array_mut) else {
                continue;
            };
            for r in refs.iter_mut() {
                let pt = r
                    .get("pattern_trips")
                    .or_else(|| r.get("schedule"))
                    .and_then(Value::as_str);
                let start = r.get("start_secs").and_then(Value::as_u64);
                if let (Some(pt), Some(start)) = (pt, start) {
                    *r = json!({"trip": format!("{pt}@{start}")});
                }
            }
        }
    }
    Ok(())
}

fn old_starts(departures: &Value) -> Vec<u32> {
    let num = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64).map(|n| n as u32);
    let mut starts: Vec<u32> = match departures.get("kind").and_then(Value::as_str) {
        Some("exact_times") => departures
            .get("starts")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_u64().map(|n| n as u32))
                    .collect()
            })
            .unwrap_or_default(),
        Some("headway_bands") => departures
            .get("bands")
            .and_then(Value::as_array)
            .map(|bands| {
                let mut out = Vec::new();
                for b in bands {
                    let (Some(start), Some(end), Some(headway)) = (
                        num(b, "start_secs"),
                        num(b, "end_secs"),
                        num(b, "headway_secs"),
                    ) else {
                        continue;
                    };
                    if headway == 0 {
                        continue;
                    }
                    let mut t = start;
                    while t < end {
                        out.push(t);
                        t += headway;
                    }
                }
                out
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    starts.sort_unstable();
    starts.dedup();
    starts
}

/// No covering profile leaves every stop unscheduled. Validation names the
/// trip.
fn profile_times(pattern: &Pattern, start: u32) -> Vec<StopOffset> {
    let profile = pattern.profile_for(start);
    let mut k = 0;
    pattern
        .stops
        .iter()
        .map(|ps| {
            let t = match profile {
                Some(p) if ps.timepoint => {
                    k += 1;
                    p.at_timepoints.get(k - 1).copied()
                }
                _ => None,
            };
            StopOffset {
                arrival_secs: t,
                departure_secs: t,
            }
        })
        .collect()
}

fn migrate_paths(root: &mut serde_json::Map<String, Value>) {
    let mut lines: Vec<(String, Value)> = Vec::new();
    if let Some(patterns) = root.get_mut("patterns").and_then(Value::as_object_mut) {
        for (id, pattern) in patterns.iter_mut() {
            let Some(pattern) = pattern.as_object_mut() else {
                continue;
            };
            let Some(path) = pattern.remove("path") else {
                continue;
            };
            if path.is_null() {
                continue;
            }
            pattern.insert("alignment".into(), Value::String(id.clone()));
            lines.push((id.clone(), path));
        }
    }
    if lines.is_empty() {
        return;
    }
    let alignments = root.entry("alignments").or_insert_with(|| json!({}));
    if let Some(alignments) = alignments.as_object_mut() {
        for (id, path) in lines {
            alignments.insert(id, json!({ "points": path }));
        }
    }
}
