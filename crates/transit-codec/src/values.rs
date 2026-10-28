//! Text ⇄ values: how the feed's cells read as model values and are
//! written back. Every reader treats an empty cell as absent (the
//! feed layer already does) and every writer writes the spec's own
//! spelling.

use chrono::NaiveDate;
use transit_network::operators::Contactless;
use transit_network::StopRule;

/// GTFS time ("H:MM:SS", hours past 24 legal) → seconds.
pub(crate) fn parse_time(text: &str) -> Option<u32> {
    let mut parts = text.trim().split(':');
    let (h, m, s) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    Some(h.parse::<u32>().ok()? * 3600 + m.parse::<u32>().ok()? * 60 + s.parse::<u32>().ok()?)
}

/// Seconds → GTFS time, "HH:MM:SS".
pub(crate) fn format_time(secs: u32) -> String {
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

pub(crate) fn time(cell: &Option<String>) -> Option<u32> {
    cell.as_deref().and_then(parse_time)
}

pub(crate) fn time_cell(secs: Option<u32>) -> Option<String> {
    secs.map(format_time)
}

pub(crate) fn parse_date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.trim(), "%Y%m%d").ok()
}

pub(crate) fn format_date(date: NaiveDate) -> String {
    date.format("%Y%m%d").to_string()
}

pub(crate) fn parse_f64(text: &Option<String>) -> Option<f64> {
    text.as_deref().and_then(|t| t.trim().parse::<f64>().ok()).filter(|v| v.is_finite())
}

pub(crate) fn float_cell(v: Option<f64>) -> Option<String> {
    v.map(|v| v.to_string())
}

/// An integer cell, whatever its spelling ("01" is 1).
pub(crate) fn int<T: std::str::FromStr>(cell: &Option<String>) -> Option<T> {
    cell.as_deref().and_then(|t| t.trim().parse().ok())
}

pub(crate) fn int_cell<T: ToString>(v: Option<T>) -> Option<String> {
    v.map(|v| v.to_string())
}

pub(crate) fn flag(cell: &Option<String>) -> bool {
    int::<u8>(cell) == Some(1)
}

pub(crate) fn flag_cell(v: bool) -> Option<String> {
    v.then(|| "1".to_string())
}

/// A required 0/1 cell.
pub(crate) fn bit_cell(v: bool) -> Option<String> {
    Some(if v { "1" } else { "0" }.to_string())
}

/// The spec's yes/no/unknown cells: 1 yes, 2 no, 0 or empty unknown.
pub(crate) fn yes_no(cell: &Option<String>) -> Option<bool> {
    match int::<u8>(cell) {
        Some(1) => Some(true),
        Some(2) => Some(false),
        _ => None,
    }
}

pub(crate) fn yes_no_cell(v: Option<bool>) -> Option<String> {
    v.map(|v| if v { "1" } else { "2" }.to_string())
}

/// A 0/1 cell with no default: absent stays absent.
pub(crate) fn opt_bit(cell: &Option<String>) -> Option<bool> {
    int::<u8>(cell).map(|v| v == 1)
}

pub(crate) fn opt_bit_cell(v: Option<bool>) -> Option<String> {
    v.map(|v| if v { "1" } else { "0" }.to_string())
}

/// Pickup / drop-off and continuous stopping codes.
pub(crate) fn stop_rule(cell: &Option<String>) -> Option<StopRule> {
    match int::<u8>(cell)? {
        0 => Some(StopRule::Regular),
        1 => Some(StopRule::NotAvailable),
        2 => Some(StopRule::PhoneAgency),
        3 => Some(StopRule::AskDriver),
        _ => None,
    }
}

pub(crate) fn stop_rule_code(rule: StopRule) -> String {
    match rule {
        StopRule::Regular => "0",
        StopRule::NotAvailable => "1",
        StopRule::PhoneAgency => "2",
        StopRule::AskDriver => "3",
    }
    .to_string()
}

/// A pickup / drop-off cell: empty means regular.
pub(crate) fn boarding_cell(rule: StopRule) -> Option<String> {
    (!rule.is_regular()).then(|| stop_rule_code(rule))
}

pub(crate) fn text(cell: &Option<String>) -> Option<String> {
    cell.clone()
}

/// A route's cEMV cell: empty means the operator's answer, so 0 is
/// kept apart from it.
pub(crate) fn contactless(cell: &Option<String>) -> Option<Contactless> {
    match int::<u8>(cell)? {
        1 => Some(Contactless::Accepted),
        2 => Some(Contactless::NotAccepted),
        _ => Some(Contactless::Unknown),
    }
}

pub(crate) fn contactless_code(c: Contactless) -> String {
    match c {
        Contactless::Unknown => "0",
        Contactless::Accepted => "1",
        Contactless::NotAccepted => "2",
    }
    .to_string()
}
