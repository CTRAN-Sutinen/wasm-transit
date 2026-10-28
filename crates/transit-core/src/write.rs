//! The other half of the codec: write a [`Gtfs`] back out as the
//! bytes of a GTFS static `.zip`.
//!
//! The parse direction ([`crate::parse`]) is one half of a codec;
//! this is the other. The producer is the network materializer (an
//! authored network document expanded into the derived-feed view —
//! the planner's publish path), but it answers over any feed the
//! model can hold. What the model carries, the writer writes; what
//! it doesn't, it omits (the file, and any column that is `None`
//! everywhere).
//!
//! Deterministic BY DESIGN: entity order is sorted by id (HashMap
//! order must not leak), zip metadata is fixed (no timestamps), and
//! the compression is a pure function of the bytes — so the same
//! feed serializes to the same zip on every host, and the zip's
//! content hash is a document identity (the capability-bus rule).

use chrono::NaiveDate;

use crate::model::{FeedInfo, ShapePoint};
use crate::{Gtfs, ServiceCalendar, fmt_time};

/// Serialize the feed as the bytes of a GTFS `.zip`. Deterministic:
/// the same [`Gtfs`] yields the same bytes everywhere.
pub fn to_zip_bytes(gtfs: &Gtfs) -> Vec<u8> {
    let mut zip = ZipWriter::new();
    write_table(&mut zip, "agency.txt", agency_rows(gtfs));
    write_table(&mut zip, "routes.txt", route_rows(gtfs));
    write_table(&mut zip, "stops.txt", stop_rows(gtfs));
    write_table(&mut zip, "trips.txt", trip_rows(gtfs));
    write_table(&mut zip, "stop_times.txt", stop_time_rows(gtfs));
    if !gtfs.services.regular.is_empty() {
        write_table(&mut zip, "calendar.txt", calendar_rows(&gtfs.services));
    }
    if !gtfs.services.exceptions.is_empty() {
        write_table(&mut zip, "calendar_dates.txt", calendar_date_rows(&gtfs.services));
    }
    if !gtfs.shapes.is_empty() {
        write_table(&mut zip, "shapes.txt", shape_rows(gtfs));
    }
    if !gtfs.transfers.is_empty() {
        write_table(&mut zip, "transfers.txt", transfer_rows(gtfs));
    }
    if let Some(info) = &gtfs.feed_info {
        write_table(&mut zip, "feed_info.txt", feed_info_rows(info));
    }
    zip.finish()
}

// ---------------------------------------------------------------------
// The zip: fixed metadata, fixed order — identity by content hash.
// ---------------------------------------------------------------------

/// A zip whose bytes carry no wall clock: the DOS epoch (mtime 0 →
/// 1980-01-01) pins every entry's timestamp, so the output is a pure
/// function of the content.
struct ZipWriter {
    writer: zip::ZipWriter<std::io::Cursor<Vec<u8>>>,
}

impl ZipWriter {
    fn new() -> ZipWriter {
        ZipWriter { writer: zip::ZipWriter::new(std::io::Cursor::new(Vec::new())) }
    }

    fn add(&mut self, name: &str, contents: &[u8]) {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        self.writer
            .start_file(name, options)
            .expect("zip entry header serializes");
        use std::io::Write;
        self.writer.write_all(contents).expect("zip entry body serializes");
    }

    fn finish(self) -> Vec<u8> {
        self.writer.finish().expect("zip central directory serializes").into_inner()
    }
}

fn write_table(zip: &mut ZipWriter, name: &str, rows: Vec<Vec<String>>) {
    let mut text = String::new();
    for row in &rows {
        text.push_str(&csv_line(row));
    }
    zip.add(name, text.as_bytes());
}

/// One CSV line (RFC 4180 quoting, `\r\n` per the GTFS spec) with a
/// trailing newline. `rows` carry the header first.
fn csv_line(row: &[String]) -> String {
    let mut out = String::new();
    for (i, cell) in row.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        if cell.contains(',') || cell.contains('"') || cell.contains('\n') || cell.contains('\r') {
            out.push('"');
            out.push_str(&cell.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(cell);
        }
    }
    out.push_str("\r\n");
    out
}

fn opt(value: &Option<String>) -> String {
    value.clone().unwrap_or_default()
}

fn time(value: Option<u32>) -> String {
    value.map(fmt_time).unwrap_or_default()
}

fn date(value: NaiveDate) -> String {
    value.format("%Y%m%d").to_string()
}

fn coord(value: Option<f64>) -> String {
    value.map(|v| format!("{v}")).unwrap_or_default()
}

fn flag(value: bool) -> String {
    if value { "1".into() } else { "0".into() }
}

/// Collect an iterator into sorted order by a string key — every
/// HashMap iteration goes through here so order never leaks.
fn sorted_by<T>(items: impl Iterator<Item = T>, key: impl Fn(&T) -> String) -> Vec<T> {
    let mut items: Vec<T> = items.collect();
    items.sort_by(|a, b| key(a).cmp(&key(b)));
    items
}

// ---------------------------------------------------------------------
// One writer function per GTFS file. Sorted by id — HashMap order
// must not leak into the bytes.
// ---------------------------------------------------------------------

fn agency_rows(gtfs: &Gtfs) -> Vec<Vec<String>> {
    let mut rows = vec![vec!["agency_id".into(), "agency_name".into(), "agency_url".into(), "agency_timezone".into()]];
    for a in sorted_by(gtfs.agencies.iter(), |a| a.agency_name.clone()) {
        rows.push(vec![opt(&a.agency_id), a.agency_name.clone(), a.agency_url.clone(), a.agency_timezone.clone()]);
    }
    rows
}

fn route_rows(gtfs: &Gtfs) -> Vec<Vec<String>> {
    let mut rows = vec![
        vec!["route_id".into(), "agency_id".into(), "route_short_name".into(), "route_long_name".into(), "route_type".into(), "route_color".into(), "route_text_color".into()],
    ];
    for r in sorted_by(gtfs.routes.values(), |r| r.route_id.clone()) {
        rows.push(vec![
            r.route_id.clone(),
            opt(&r.agency_id),
            opt(&r.route_short_name),
            opt(&r.route_long_name),
            r.route_type.to_string(),
            opt(&r.route_color),
            opt(&r.route_text_color),
        ]);
    }
    rows
}

fn stop_rows(gtfs: &Gtfs) -> Vec<Vec<String>> {
    let mut rows = vec![
        vec!["stop_id".into(), "stop_code".into(), "stop_name".into(), "stop_lat".into(), "stop_lon".into(), "location_type".into(), "parent_station".into()],
    ];
    for s in sorted_by(gtfs.stops.values(), |s| s.stop_id.clone()) {
        rows.push(vec![
            s.stop_id.clone(),
            opt(&s.stop_code),
            s.stop_name.clone(),
            coord(s.stop_lat),
            coord(s.stop_lon),
            s.location_type.map(|t| t.to_string()).unwrap_or_default(),
            opt(&s.parent_station),
        ]);
    }
    rows
}

fn trip_rows(gtfs: &Gtfs) -> Vec<Vec<String>> {
    let mut rows = vec![
        vec!["route_id".into(), "service_id".into(), "trip_id".into(), "trip_headsign".into(), "direction_id".into(), "block_id".into(), "shape_id".into()],
    ];
    for t in sorted_by(gtfs.trips.values(), |t| t.trip_id.clone()) {
        rows.push(vec![
            t.route_id.clone(),
            t.service_id.clone(),
            t.trip_id.clone(),
            opt(&t.trip_headsign),
            t.direction_id.map(|d| d.to_string()).unwrap_or_default(),
            opt(&t.block_id),
            opt(&t.shape_id),
        ]);
    }
    rows
}

fn stop_time_rows(gtfs: &Gtfs) -> Vec<Vec<String>> {
    let mut rows = vec![
        vec!["trip_id".into(), "arrival_time".into(), "departure_time".into(), "stop_id".into(), "stop_sequence".into(), "stop_headsign".into(), "timepoint".into(), "shape_dist_traveled".into()],
    ];
    // The vec order is the feed's own order (parse kept stop_times
    // in file order; the materializer emits schedules in id order).
    for st in &gtfs.stop_times {
        rows.push(vec![
            st.trip_id.clone(),
            time(st.arrival_time),
            time(st.departure_time),
            st.stop_id.clone(),
            st.stop_sequence.to_string(),
            opt(&st.stop_headsign),
            if st.timepoint { "1".into() } else { "0".into() },
            st.shape_dist_traveled.map_or_else(String::new, |d| format!("{d}")),
        ]);
    }
    rows
}

fn calendar_rows(services: &ServiceCalendar) -> Vec<Vec<String>> {
    let mut rows = vec![
        vec!["service_id".into(), "monday".into(), "tuesday".into(), "wednesday".into(), "thursday".into(), "friday".into(), "saturday".into(), "sunday".into(), "start_date".into(), "end_date".into()],
    ];
    for c in sorted_by(services.regular.values(), |c| c.service_id.clone()) {
        rows.push(vec![
            c.service_id.clone(),
            flag(c.monday),
            flag(c.tuesday),
            flag(c.wednesday),
            flag(c.thursday),
            flag(c.friday),
            flag(c.saturday),
            flag(c.sunday),
            date(c.start_date),
            date(c.end_date),
        ]);
    }
    rows
}

fn calendar_date_rows(services: &ServiceCalendar) -> Vec<Vec<String>> {
    let mut rows = vec![vec!["service_id".into(), "date".into(), "exception_type".into()]];
    // A BTree keyed on (service_id, date): exceptions live in a
    // HashMap, so sort here — determinism is the writer's contract.
    let mut entries = std::collections::BTreeMap::new();
    for (key, kind) in &services.exceptions {
        entries.insert(key.clone(), *kind);
    }
    for ((service_id, day), kind) in entries {
        rows.push(vec![service_id.clone(), date(day), kind.to_string()]);
    }
    rows
}

fn shape_rows(gtfs: &Gtfs) -> Vec<Vec<String>> {
    let mut rows = vec![
        vec!["shape_id".into(), "shape_pt_lat".into(), "shape_pt_lon".into(), "shape_pt_sequence".into(), "shape_dist_traveled".into()],
    ];
    let mut points: Vec<&ShapePoint> = gtfs.shapes.values().flatten().collect();
    points.sort_by(|a, b| a.shape_id.cmp(&b.shape_id).then(a.shape_pt_sequence.cmp(&b.shape_pt_sequence)));
    for p in points {
        rows.push(vec![
            p.shape_id.clone(),
            format!("{}", p.shape_pt_lat),
            format!("{}", p.shape_pt_lon),
            p.shape_pt_sequence.to_string(),
            p.shape_dist_traveled.map(|d| format!("{d}")).unwrap_or_default(),
        ]);
    }
    rows
}

fn transfer_rows(gtfs: &Gtfs) -> Vec<Vec<String>> {
    let mut rows = vec![
        vec!["from_stop_id".into(), "to_stop_id".into(), "transfer_type".into(), "min_transfer_time".into()],
    ];
    for t in &gtfs.transfers {
        rows.push(vec![
            t.from_stop_id.clone(),
            t.to_stop_id.clone(),
            t.transfer_type.map(|k| k.to_string()).unwrap_or_default(),
            t.min_transfer_time.map(|m| m.to_string()).unwrap_or_default(),
        ]);
    }
    rows
}

fn feed_info_rows(info: &FeedInfo) -> Vec<Vec<String>> {
    vec![
        vec!["feed_id".into(), "feed_version".into(), "feed_publisher_name".into(), "feed_publisher_url".into(), "feed_lang".into()],
        vec![
            opt(&info.feed_id),
            opt(&info.feed_version),
            opt(&info.feed_publisher_name),
            opt(&info.feed_publisher_url),
            opt(&info.feed_lang),
        ],
    ]
}

/// Standard base64 (RFC 4648, padded). Bulk payloads cross JSON
/// seams encoded (a JSON array of numbers would be absurd); hosts
/// decode with any standard decoder.
pub fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { ALPHABET[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[n as usize & 63] as char } else { '=' });
    }
    out
}

// ---------------------------------------------------------------------
// Tests: the writer's three laws — the round trip preserves the feed,
// the bytes are deterministic (identity by content hash), and the
// JSON-seam encoding is standard base64.
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Round trip through the writer and the parser: the fixture
    /// feed's entities survive byte-level serialization.
    #[test]
    fn round_trips_the_fixture() {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/e2e/fixtures/sample-gtfs.zip"))
            .expect("fixture exists");
        let gtfs = crate::Gtfs::from_zip_bytes(&bytes).expect("fixture parses");
        let out = to_zip_bytes(&gtfs);
        let reparsed = crate::Gtfs::from_zip_bytes(&out).expect("written zip parses");
        let (a, b) = (reparsed.summary(), gtfs.summary());
        assert_eq!(
            (a.agencies, a.routes, a.stops, a.trips, a.stop_times),
            (b.agencies, b.routes, b.stops, b.trips, b.stop_times),
            "entity counts survive the round trip"
        );
        for (stop_id, stop) in &gtfs.stops {
            let back = reparsed.stops.get(stop_id).expect("stop survives");
            assert_eq!(back.stop_name, stop.stop_name);
            assert_eq!(back.stop_lat, stop.stop_lat, "coordinates round-trip exactly");
            assert_eq!(back.stop_lon, stop.stop_lon);
        }
        for (trip_id, trip) in &gtfs.trips {
            let back = reparsed.trips.get(trip_id).expect("trip survives");
            assert_eq!(back.route_id, trip.route_id);
            assert_eq!(back.block_id, trip.block_id);
            assert_eq!(back.shape_id, trip.shape_id);
        }
    }

    /// Determinism: the same feed serializes to the same bytes twice.
    #[test]
    fn deterministic_bytes() {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/e2e/fixtures/sample-gtfs.zip"))
            .expect("fixture exists");
        let gtfs = crate::Gtfs::from_zip_bytes(&bytes).expect("fixture parses");
        assert_eq!(to_zip_bytes(&gtfs), to_zip_bytes(&gtfs), "identical input, identical zip bytes");
    }

    /// RFC 4648 test vectors.
    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(super::base64_encode(b""), "");
        assert_eq!(super::base64_encode(b"f"), "Zg==");
        assert_eq!(super::base64_encode(b"fo"), "Zm8=");
        assert_eq!(super::base64_encode(b"foo"), "Zm9v");
        assert_eq!(super::base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(super::base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(super::base64_encode(b"foobar"), "Zm9vYmFy");
    }
}
