//! Every reader treats an empty cell as absent. Every writer uses the
//! spec's spelling.

use transit_date::Date;
use transit_network::operators::Contactless;
use transit_network::StopRule;

/// GTFS time ("H:MM:SS"). Hours past 24 are legal.
pub(crate) fn parse_time(text: &str) -> Option<u32> {
    let mut parts = text.trim().split(':');
    let (h, m, s) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    Some(h.parse::<u32>().ok()? * 3600 + m.parse::<u32>().ok()? * 60 + s.parse::<u32>().ok()?)
}

pub(crate) fn format_time(secs: u32) -> String {
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

pub(crate) fn time(cell: &Option<String>) -> Option<u32> {
    cell.as_deref().and_then(parse_time)
}

pub(crate) fn time_cell(secs: Option<u32>) -> Option<String> {
    secs.map(format_time)
}

/// GTFS's "YYYYMMDD", the date's form at the boundary. Spaces around the
/// cell are dropped; anything else that is not eight digits naming a day
/// is no date.
pub fn parse_gtfs_date(text: &str) -> Option<Date> {
    let t = text.trim();
    if t.len() != 8 || !t.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Date::from_ymd_opt(t[0..4].parse().ok()?, t[4..6].parse().ok()?, t[6..8].parse().ok()?)
}

pub(crate) fn format_date(date: Date) -> String {
    format!("{:04}{:02}{:02}", date.year(), date.month(), date.day())
}

pub(crate) fn parse_f64(text: &Option<String>) -> Option<f64> {
    text.as_deref().and_then(|t| t.trim().parse::<f64>().ok()).filter(|v| v.is_finite())
}

pub(crate) fn float_cell(v: Option<f64>) -> Option<String> {
    v.map(|v| v.to_string())
}

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

pub(crate) fn bit_cell(v: bool) -> Option<String> {
    Some(if v { "1" } else { "0" }.to_string())
}

/// 1 is yes, 2 is no, 0 or empty is unknown.
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

pub(crate) fn opt_bit(cell: &Option<String>) -> Option<bool> {
    int::<u8>(cell).map(|v| v == 1)
}

pub(crate) fn opt_bit_cell(v: Option<bool>) -> Option<String> {
    v.map(|v| if v { "1" } else { "0" }.to_string())
}

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

pub(crate) fn boarding_cell(rule: StopRule) -> Option<String> {
    (!rule.is_regular()).then(|| stop_rule_code(rule))
}

pub(crate) fn text(cell: &Option<String>) -> Option<String> {
    cell.clone()
}

/// Empty means the operator's answer, so 0 (unknown) stays apart from it.
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
