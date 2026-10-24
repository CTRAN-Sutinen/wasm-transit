//! GTFS time fields.
//!
//! Times are written `HH:MM:SS` — but unlike a wall clock, the hour may
//! exceed 23: a trip at "25:30:00" departs 1.5h after midnight of the
//! *service day* it belongs to. We therefore store all times as plain
//! seconds since service-day midnight (`u32`), never as a clock type.

/// Parse a GTFS time string into seconds since service-day midnight.
/// Returns `None` for malformed values (which are common in real feeds).
pub fn parse_time(s: &str) -> Option<u32> {
    let mut parts = s.trim().splitn(3, ':');
    let h: u32 = parts.next()?.trim().parse().ok()?;
    let m: u32 = parts.next()?.trim().parse().ok()?;
    let sec: u32 = parts.next()?.trim().parse().ok()?;
    if m > 59 || sec > 59 {
        return None;
    }
    Some(h * 3600 + m * 60 + sec)
}

/// Render seconds since midnight back to GTFS `HH:MM:SS` form.
pub fn fmt_time(secs: u32) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_normal_times() {
        assert_eq!(parse_time("08:00:00"), Some(8 * 3600));
        assert_eq!(parse_time("23:59:59"), Some(24 * 3600 - 1));
    }

    #[test]
    fn parses_after_midnight_times() {
        // 1h30 past midnight of the service day
        assert_eq!(parse_time("25:30:00"), Some(25 * 3600 + 30 * 60));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_time("8:00"), None);
        assert_eq!(parse_time("08:60:00"), None);
        assert_eq!(parse_time(""), None);
        assert_eq!(parse_time("nonsense"), None);
    }

    #[test]
    fn roundtrips_through_fmt() {
        assert_eq!(fmt_time(25 * 3600 + 30 * 60), "25:30:00");
        assert_eq!(fmt_time(0), "00:00:00");
    }
}
