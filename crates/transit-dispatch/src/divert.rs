//! Authoring a diversion: the editor's draft, answered over the
//! NETWORK.
//!
//! A dispatcher builds a diversion in steps — a route, the pattern
//! (the model's direction), two anchors, which interior stops the
//! detour still serves, a traced path, added time, dates. The DRAFT
//! is that form state, plain data the host holds; the VIEW is
//! everything the editor shows about it (the route's patterns, the
//! default pattern, each stop's role, the skips in effect, the
//! alignment the detour is drawn against, the traced path and its
//! length) and whether it is ready to become a record. The record
//! itself is [`diversion_of`] — the same rule for every host.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use transit_network::{Coord, Network};
use transit_sync::{DetourPoint, Diversion, EditRecord, EditStatus};

/// The editor's form state. Every field but the route may be unset
/// while the dispatcher works.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DiversionDraft {
    pub route_id: String,
    /// The pattern anchored against; unset (or not the route's) means
    /// the default — the route's longest pattern.
    #[serde(default)]
    #[ts(optional)]
    pub pattern: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub first_kept_stop: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub last_kept_stop: Option<String>,
    /// Interior stops the detour still SERVES. Every other stop between
    /// the anchors is skipped — a detour usually bypasses them — so new
    /// anchors never leave stale skips behind.
    #[serde(default)]
    pub served_stops: Vec<String>,
    /// The traced path, one slice per waypoint step (Undo removes a
    /// whole step); flattened into the record's detour.
    #[serde(default)]
    pub slices: Vec<Vec<DetourPoint>>,
    /// Extra running time, in the form's unit (minutes).
    #[serde(default)]
    pub added_mins: f64,
    /// "YYYY-MM-DD".
    pub start_date: String,
    pub end_date: String,
}

/// One of the route's patterns, as the picker offers it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DraftPattern {
    pub pattern: String,
    pub name: String,
    pub stop_count: u32,
}

/// A stop's part in the draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum DraftStopRole {
    FirstKept,
    LastKept,
    /// Strictly between the anchors: skippable.
    Interior,
    Outside,
}

/// One visit of the pattern (a loop visits a stop twice — `index` is
/// the identity).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DraftStop {
    pub index: u32,
    pub stop_id: String,
    pub name: String,
    pub role: DraftStopRole,
    /// In effect: an interior stop the detour does not serve.
    pub skipped: bool,
}

/// What the editor shows about a draft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DraftView {
    /// The route's patterns, in the network's order.
    pub patterns: Vec<DraftPattern>,
    /// The pattern in effect: the draft's when it is the route's, else
    /// the longest (ties: the first).
    pub pattern: Option<String>,
    pub stops: Vec<DraftStop>,
    /// The skips the record would carry, in pattern order.
    pub skipped_stops: Vec<String>,
    /// The pattern's alignment — its authored path, stop-to-stop when
    /// none is authored: the line the detour is drawn against.
    pub alignment: Vec<Coord>,
    /// The traced path, flattened (consecutive duplicates dropped —
    /// routing can return the previous endpoint).
    pub detour: Vec<DetourPoint>,
    pub detour_length_m: f64,
    /// Why the draft cannot become a record yet; empty = ready.
    pub problems: Vec<String>,
}

/// The view of a draft over the network. Total: a draft that names
/// things the network lacks answers problems, never an error.
pub fn draft_view(network: &Network, draft: &DiversionDraft) -> DraftView {
    let mut problems = Vec::new();
    let patterns: Vec<DraftPattern> = network
        .patterns
        .iter()
        .filter(|(_, p)| p.route == draft.route_id)
        .map(|(id, p)| DraftPattern { pattern: id.clone(), name: p.name.clone(), stop_count: p.stops.len() as u32 })
        .collect();
    let pattern = draft
        .pattern
        .as_ref()
        .filter(|id| patterns.iter().any(|p| &p.pattern == *id))
        .cloned()
        .or_else(|| {
            patterns
                .iter()
                .fold(None::<&DraftPattern>, |best, p| match best {
                    Some(b) if b.stop_count >= p.stop_count => Some(b),
                    _ => Some(p),
                })
                .map(|p| p.pattern.clone())
        });
    if draft.route_id.is_empty() {
        problems.push("pick a route".into());
    } else if !network.routes.contains_key(&draft.route_id) {
        problems.push(format!("unknown route {}", draft.route_id));
    } else if pattern.is_none() {
        problems.push("the route has no patterns".into());
    }

    let visits: Vec<&str> = pattern
        .as_ref()
        .and_then(|id| network.patterns.get(id))
        .map(|p| p.stops.iter().map(|ps| ps.stop.as_str()).collect())
        .unwrap_or_default();
    let first = draft.first_kept_stop.as_deref().filter(|s| !s.is_empty());
    let last = draft.last_kept_stop.as_deref().filter(|s| !s.is_empty());
    // The anchors the patch will match: the first visit of the first
    // anchor, the first visit of the last anchor after it.
    let p0 = first.and_then(|f| visits.iter().position(|s| *s == f));
    let p1 = p0.zip(last).and_then(|(p0, l)| visits[p0 + 1..].iter().position(|s| *s == l).map(|i| i + p0 + 1));
    if pattern.is_some() {
        match (first, last) {
            (None, _) => problems.push("pick the first stop the diversion keeps".into()),
            (Some(f), _) if p0.is_none() => problems.push(format!("{} is not on this pattern", stop_name(network, f))),
            (Some(_), None) => problems.push("pick the last stop the diversion keeps".into()),
            (Some(_), Some(l)) if p1.is_none() => {
                problems.push(format!("{} is not after the first kept stop on this pattern", stop_name(network, l)))
            }
            _ => {}
        }
    }

    let stops: Vec<DraftStop> = visits
        .iter()
        .enumerate()
        .map(|(i, stop)| {
            let role = if Some(i) == p0 {
                DraftStopRole::FirstKept
            } else if Some(i) == p1 {
                DraftStopRole::LastKept
            } else if p0.zip(p1).is_some_and(|(a, b)| a < i && i < b) {
                DraftStopRole::Interior
            } else {
                DraftStopRole::Outside
            };
            let skipped = role == DraftStopRole::Interior && !draft.served_stops.iter().any(|s| s == stop);
            DraftStop { index: i as u32, stop_id: stop.to_string(), name: stop_name(network, stop), role, skipped }
        })
        .collect();
    let mut skipped_stops: Vec<String> = Vec::new();
    for s in stops.iter().filter(|s| s.skipped) {
        // An anchor visited again inside the span is still an anchor:
        // the record must never skip it.
        if !skipped_stops.contains(&s.stop_id) && Some(s.stop_id.as_str()) != first && Some(s.stop_id.as_str()) != last {
            skipped_stops.push(s.stop_id.clone());
        }
    }

    let alignment = pattern.as_ref().map(|id| transit_network::pattern_alignment(network, id)).unwrap_or_default();
    let mut detour: Vec<DetourPoint> = Vec::new();
    for p in draft.slices.iter().flatten() {
        if detour.last().is_some_and(|q| (q.lat - p.lat).abs() < 1e-12 && (q.lon - p.lon).abs() < 1e-12) {
            continue;
        }
        detour.push(*p);
    }
    let detour_length_m = path_length_m(&detour);

    if !(draft.added_mins.is_finite() && draft.added_mins >= 0.0) {
        problems.push("added time must be zero or more minutes".into());
    }
    match (parse_date(&draft.start_date), parse_date(&draft.end_date)) {
        (Some(a), Some(b)) if a > b => problems.push("the start date is after the end date".into()),
        (Some(_), Some(_)) => {}
        _ => problems.push("both dates must be YYYY-MM-DD".into()),
    }

    DraftView { patterns, pattern, stops, skipped_stops, alignment, detour, detour_length_m, problems }
}

/// The diversion a ready draft becomes: anchors, the skips in effect,
/// the traced path (none below two points — skip only), the added
/// time in seconds, the dates. `id` is the domain id the host mints
/// (the record id is the sync envelope's). Errors name every problem.
pub fn diversion_of(network: &Network, draft: &DiversionDraft, id: &str) -> Result<Diversion, String> {
    let view = draft_view(network, draft);
    if !view.problems.is_empty() {
        return Err(view.problems.join("; "));
    }
    Ok(Diversion {
        id: id.to_string(),
        route_id: draft.route_id.clone(),
        // The pattern owns the anchor truth; GTFS's direction_id is
        // feed vocabulary the model never matches on.
        direction_id: None,
        first_kept_stop: draft.first_kept_stop.clone().unwrap_or_default(),
        last_kept_stop: draft.last_kept_stop.clone().unwrap_or_default(),
        skipped_stops: view.skipped_stops,
        detour: (view.detour.len() >= 2).then_some(view.detour),
        added_secs: (draft.added_mins * 60.0).round() as u32,
        start_date: parse_date(&draft.start_date).expect("checked by the view"),
        end_date: parse_date(&draft.end_date).expect("checked by the view"),
    })
}

/// One record of the edit document as the dispatcher's list shows it:
/// ids resolved to the network's names (an id the network lacks shows
/// as itself — the record's own validation report says why).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DiversionRow {
    pub record_id: String,
    pub status: EditStatus,
    /// Retired is the tombstone: out of the working list.
    pub in_force: bool,
    pub route_name: String,
    pub first_kept_name: String,
    pub last_kept_name: String,
    pub skipped_names: Vec<String>,
    pub added_secs: u32,
    pub detour_drawn: bool,
    pub start_date: String,
    pub end_date: String,
}

/// The edit document's rows, in document order.
pub fn diversion_rows(network: &Network, records: &[EditRecord]) -> Vec<DiversionRow> {
    records
        .iter()
        .map(|r| {
            let d = &r.diversion;
            DiversionRow {
                record_id: r.id.clone(),
                status: r.status,
                in_force: r.status != EditStatus::Retired,
                route_name: network.routes.get(&d.route_id).map(|x| x.name.clone()).unwrap_or_else(|| d.route_id.clone()),
                first_kept_name: stop_name(network, &d.first_kept_stop),
                last_kept_name: stop_name(network, &d.last_kept_stop),
                skipped_names: d.skipped_stops.iter().map(|s| stop_name(network, s)).collect(),
                added_secs: d.added_secs,
                detour_drawn: d.detour.as_ref().is_some_and(|p| !p.is_empty()),
                start_date: d.start_date.format("%Y-%m-%d").to_string(),
                end_date: d.end_date.format("%Y-%m-%d").to_string(),
            }
        })
        .collect()
}

fn stop_name(network: &Network, stop: &str) -> String {
    network.stops.get(stop).map(|s| s.name.clone()).unwrap_or_else(|| stop.to_string())
}

fn parse_date(raw: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok()
}

/// Path length in metres (equirectangular per leg — exact enough at
/// street scale; what the dispatcher reasons about for added time).
fn path_length_m(points: &[DetourPoint]) -> f64 {
    points
        .windows(2)
        .map(|w| {
            let cos_lat = ((w[0].lat + w[1].lat) / 2.0).to_radians().cos();
            ((w[1].lon - w[0].lon) * 111_320.0 * cos_lat).hypot((w[1].lat - w[0].lat) * 110_540.0)
        })
        .sum()
}
