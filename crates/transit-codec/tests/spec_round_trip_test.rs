//! The spec round-trip law: for every spec-valid GTFS feed,
//! `export(import(feed))` is the SAME feed — over the whole GTFS
//! Schedule reference, every file and every field.
//!
//! This is the law that sees import-side loss. The document fixpoint
//! (`codec_prop_test.rs`) holds `import(export(import(F))) ==
//! import(F)`, which cannot fail on anything the import drops: a
//! dropped field never enters the first document. Here the oracle is
//! the feed itself.
//!
//! The oracle is independent of the code under test: both zips are
//! read as RAW CSV tables against the spec table below — never
//! through a parser the codec owns — so a file or column the parser
//! ignores is a loss here, not an invisible one. The generator writes
//! every file and field the spec defines, with the cross-file
//! references and conditional requirements the spec states.
//!
//! What "the same feed" means — the only equivalences allowed:
//!
//! * CSV encoding: file order, row order, COLUMN order, quoting, line
//!   endings, a BOM. A row is a map from column to value.
//! * An empty cell is an absent cell (the spec's own reading of an
//!   optional field), and so is a cell holding the value the spec
//!   says empty MEANS ("0 or empty", "1 or empty" — `SPEC_DEFAULTS`).
//!   Only where the spec says so: an empty `fare_attributes.transfers`
//!   is unlimited, not 0, and an empty stop-time continuous_* inherits
//!   the route's, so neither has a default here.
//! * Typed values compare by value: integers and enums as integers,
//!   floats as f64 (`45.50` ≡ `45.5`), times as seconds (`8:00:00` ≡
//!   `08:00:00`), colors without case. Text, ids, URLs, and dates
//!   compare exactly.
//! * `locations.geojson` compares as a JSON value.
//!
//! Nothing else. Ids survive (trip, service, shape, fare — realtime
//! feeds and every downstream consumer key on them); a calendar
//! published as calendar.txt stays calendar.txt; two trips with
//! identical times are two trips.
//!
//! One law per file, so the failing set is the codec's to-do list:
//! a proptest failure shrinks `families` and `density` toward the
//! smallest feed that loses something from that file.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use proptest::prelude::*;

// ===================================================================
// The codec's round trip — the ONLY lines that name the codec's API.
// When the boundary moves (the import keeps a residual beside the
// network, transit-core folds into the codec), only this changes.
// ===================================================================

fn round_trip(feed: &[u8]) -> Result<Vec<u8>, String> {
    let feed = transit_codec::Feed::from_zip_bytes(feed)
        .map_err(|e| format!("the codec cannot read a spec-valid feed: {e}"))?;
    let outcome = transit_codec::import(&feed);
    let exported = transit_codec::export(&outcome.network, &outcome.boundary)
        .map_err(|e| format!("the codec cannot export its own import: {e}"))?;
    Ok(exported.to_zip_bytes())
}

// ===================================================================
// The spec table: every file and field of the GTFS Schedule
// reference, with the type its value compares as.
// ===================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    /// Ids, names, URLs, emails, phones, timezones, languages,
    /// currency codes, dates — exact text.
    Text,
    /// Integers and enums.
    Int,
    Float,
    /// HH:MM:SS, hours may exceed 24 and may be one digit.
    Time,
    /// Six hex digits; case is not significant.
    Color,
}
use Kind::*;

struct FileSpec {
    name: &'static str,
    columns: &'static [(&'static str, Kind)],
}

const LOCATIONS: &str = "locations.geojson";

const SPEC: &[FileSpec] = &[
    FileSpec { name: "agency.txt", columns: &[
        ("agency_id", Text), ("agency_name", Text), ("agency_url", Text),
        ("agency_timezone", Text), ("agency_lang", Text), ("agency_phone", Text),
        ("agency_fare_url", Text), ("agency_email", Text), ("cemv_support", Int),
    ] },
    FileSpec { name: "stops.txt", columns: &[
        ("stop_id", Text), ("stop_code", Text), ("stop_name", Text), ("tts_stop_name", Text),
        ("stop_desc", Text), ("stop_lat", Float), ("stop_lon", Float), ("zone_id", Text),
        ("stop_url", Text), ("location_type", Int), ("parent_station", Text),
        ("stop_timezone", Text), ("wheelchair_boarding", Int), ("level_id", Text),
        ("platform_code", Text), ("stop_access", Int),
    ] },
    FileSpec { name: "routes.txt", columns: &[
        ("route_id", Text), ("agency_id", Text), ("route_short_name", Text),
        ("route_long_name", Text), ("route_desc", Text), ("route_type", Int),
        ("route_url", Text), ("route_color", Color), ("route_text_color", Color),
        ("route_sort_order", Int), ("continuous_pickup", Int), ("continuous_drop_off", Int),
        ("network_id", Text), ("cemv_support", Int),
    ] },
    FileSpec { name: "trips.txt", columns: &[
        ("route_id", Text), ("service_id", Text), ("trip_id", Text), ("trip_headsign", Text),
        ("trip_short_name", Text), ("direction_id", Int), ("block_id", Text),
        ("shape_id", Text), ("wheelchair_accessible", Int), ("bikes_allowed", Int),
        ("cars_allowed", Int),
    ] },
    FileSpec { name: "stop_times.txt", columns: &[
        ("trip_id", Text), ("arrival_time", Time), ("departure_time", Time),
        ("stop_id", Text), ("location_group_id", Text), ("location_id", Text),
        ("stop_sequence", Int), ("stop_headsign", Text),
        ("start_pickup_drop_off_window", Time), ("end_pickup_drop_off_window", Time),
        ("pickup_type", Int), ("drop_off_type", Int), ("continuous_pickup", Int),
        ("continuous_drop_off", Int), ("shape_dist_traveled", Float), ("timepoint", Int),
        ("pickup_booking_rule_id", Text), ("drop_off_booking_rule_id", Text),
    ] },
    FileSpec { name: "calendar.txt", columns: &[
        ("service_id", Text), ("monday", Int), ("tuesday", Int), ("wednesday", Int),
        ("thursday", Int), ("friday", Int), ("saturday", Int), ("sunday", Int),
        ("start_date", Text), ("end_date", Text),
    ] },
    FileSpec { name: "calendar_dates.txt", columns: &[
        ("service_id", Text), ("date", Text), ("exception_type", Int),
    ] },
    FileSpec { name: "fare_attributes.txt", columns: &[
        ("fare_id", Text), ("price", Float), ("currency_type", Text), ("payment_method", Int),
        ("transfers", Int), ("agency_id", Text), ("transfer_duration", Int),
    ] },
    FileSpec { name: "fare_rules.txt", columns: &[
        ("fare_id", Text), ("route_id", Text), ("origin_id", Text),
        ("destination_id", Text), ("contains_id", Text),
    ] },
    FileSpec { name: "timeframes.txt", columns: &[
        ("timeframe_group_id", Text), ("start_time", Time), ("end_time", Time),
        ("service_id", Text),
    ] },
    FileSpec { name: "rider_categories.txt", columns: &[
        ("rider_category_id", Text), ("rider_category_name", Text),
        ("is_default_fare_category", Int), ("eligibility_url", Text),
    ] },
    FileSpec { name: "fare_media.txt", columns: &[
        ("fare_media_id", Text), ("fare_media_name", Text), ("fare_media_type", Int),
    ] },
    FileSpec { name: "fare_products.txt", columns: &[
        ("fare_product_id", Text), ("fare_product_name", Text), ("rider_category_id", Text),
        ("fare_media_id", Text), ("amount", Float), ("currency", Text),
    ] },
    FileSpec { name: "fare_leg_rules.txt", columns: &[
        ("leg_group_id", Text), ("network_id", Text), ("from_area_id", Text),
        ("to_area_id", Text), ("from_timeframe_group_id", Text),
        ("to_timeframe_group_id", Text), ("fare_product_id", Text), ("rule_priority", Int),
    ] },
    FileSpec { name: "fare_leg_join_rules.txt", columns: &[
        ("from_network_id", Text), ("to_network_id", Text), ("from_stop_id", Text),
        ("to_stop_id", Text),
    ] },
    FileSpec { name: "fare_transfer_rules.txt", columns: &[
        ("from_leg_group_id", Text), ("to_leg_group_id", Text), ("transfer_count", Int),
        ("duration_limit", Int), ("duration_limit_type", Int), ("fare_transfer_type", Int),
        ("fare_product_id", Text),
    ] },
    FileSpec { name: "areas.txt", columns: &[("area_id", Text), ("area_name", Text)] },
    FileSpec { name: "stop_areas.txt", columns: &[("area_id", Text), ("stop_id", Text)] },
    FileSpec { name: "networks.txt", columns: &[("network_id", Text), ("network_name", Text)] },
    FileSpec { name: "route_networks.txt", columns: &[("network_id", Text), ("route_id", Text)] },
    FileSpec { name: "shapes.txt", columns: &[
        ("shape_id", Text), ("shape_pt_lat", Float), ("shape_pt_lon", Float),
        ("shape_pt_sequence", Int), ("shape_dist_traveled", Float),
    ] },
    FileSpec { name: "frequencies.txt", columns: &[
        ("trip_id", Text), ("start_time", Time), ("end_time", Time), ("headway_secs", Int),
        ("exact_times", Int),
    ] },
    FileSpec { name: "transfers.txt", columns: &[
        ("from_stop_id", Text), ("to_stop_id", Text), ("from_route_id", Text),
        ("to_route_id", Text), ("from_trip_id", Text), ("to_trip_id", Text),
        ("transfer_type", Int), ("min_transfer_time", Int),
    ] },
    FileSpec { name: "pathways.txt", columns: &[
        ("pathway_id", Text), ("from_stop_id", Text), ("to_stop_id", Text),
        ("pathway_mode", Int), ("is_bidirectional", Int), ("length", Float),
        ("traversal_time", Int), ("stair_count", Int), ("max_slope", Float),
        ("min_width", Float), ("signposted_as", Text), ("reversed_signposted_as", Text),
    ] },
    FileSpec { name: "levels.txt", columns: &[
        ("level_id", Text), ("level_index", Float), ("level_name", Text),
    ] },
    FileSpec { name: "location_groups.txt", columns: &[
        ("location_group_id", Text), ("location_group_name", Text),
    ] },
    FileSpec { name: "location_group_stops.txt", columns: &[
        ("location_group_id", Text), ("stop_id", Text),
    ] },
    FileSpec { name: "booking_rules.txt", columns: &[
        ("booking_rule_id", Text), ("booking_type", Int), ("prior_notice_duration_min", Int),
        ("prior_notice_duration_max", Int), ("prior_notice_last_day", Int),
        ("prior_notice_last_time", Time), ("prior_notice_start_day", Int),
        ("prior_notice_start_time", Time), ("prior_notice_service_id", Text),
        ("message", Text), ("pickup_message", Text), ("drop_off_message", Text),
        ("phone_number", Text), ("info_url", Text), ("booking_url", Text),
    ] },
    FileSpec { name: "translations.txt", columns: &[
        ("table_name", Text), ("field_name", Text), ("language", Text),
        ("translation", Text), ("record_id", Text), ("record_sub_id", Text),
        ("field_value", Text),
    ] },
    FileSpec { name: "feed_info.txt", columns: &[
        ("feed_publisher_name", Text), ("feed_publisher_url", Text), ("feed_lang", Text),
        ("default_lang", Text), ("feed_start_date", Text), ("feed_end_date", Text),
        ("feed_version", Text), ("feed_contact_email", Text), ("feed_contact_url", Text),
    ] },
    FileSpec { name: "attributions.txt", columns: &[
        ("attribution_id", Text), ("agency_id", Text), ("route_id", Text),
        ("trip_id", Text), ("organization_name", Text), ("is_producer", Int),
        ("is_operator", Int), ("is_authority", Int), ("attribution_url", Text),
        ("attribution_email", Text), ("attribution_phone", Text),
    ] },
];

/// (file, column, the value an empty cell means) — the spec's
/// "N or empty" enums.
const SPEC_DEFAULTS: &[(&str, &str, &str)] = &[
    ("agency.txt", "cemv_support", "0"),
    ("stops.txt", "location_type", "0"),
    ("stops.txt", "wheelchair_boarding", "0"),
    ("routes.txt", "continuous_pickup", "1"),
    ("routes.txt", "continuous_drop_off", "1"),
    ("trips.txt", "wheelchair_accessible", "0"),
    ("trips.txt", "bikes_allowed", "0"),
    ("trips.txt", "cars_allowed", "0"),
    ("stop_times.txt", "pickup_type", "0"),
    ("stop_times.txt", "drop_off_type", "0"),
    // Empty means exact on a row that carries times; the generator
    // only leaves times off rows it marks approximate (0).
    ("stop_times.txt", "timepoint", "1"),
    ("frequencies.txt", "exact_times", "0"),
    ("transfers.txt", "transfer_type", "0"),
    ("rider_categories.txt", "is_default_fare_category", "0"),
    ("fare_leg_rules.txt", "rule_priority", "0"),
    ("attributions.txt", "is_producer", "0"),
    ("attributions.txt", "is_operator", "0"),
    ("attributions.txt", "is_authority", "0"),
];

fn spec(file: &str) -> &'static FileSpec {
    SPEC.iter().find(|f| f.name == file).unwrap_or_else(|| panic!("{file} is not in the spec table"))
}

// ===================================================================
// The generator: a spec-valid feed exercising every file and field.
// `families` gates the optional files (shrinks to the core schedule),
// `density` the optional fields (shrinks to required fields only);
// the seed drives every value.
// ===================================================================

const CALENDAR_DATES: u16 = 1 << 0;
const SHAPES: u16 = 1 << 1;
const FREQUENCIES: u16 = 1 << 2;
const TRANSFERS: u16 = 1 << 3;
const STATIONS: u16 = 1 << 4; // stations, entrances, nodes, boarding areas, levels, pathways
const FARES_V1: u16 = 1 << 5;
const FARES_V2: u16 = 1 << 6;
const FLEX: u16 = 1 << 7; // location groups, locations.geojson, booking rules, flex trips
const TRANSLATIONS: u16 = 1 << 8; // requires feed_info (the spec's own rule)
const FEED_INFO: u16 = 1 << 9;
const ATTRIBUTIONS: u16 = 1 << 10;
const TWO_AGENCIES: u16 = 1 << 11;
const ALL_FAMILIES: u16 = 1 << 12;

/// One scheduled trip: the adversarial core of `import_prop_test.rs`
/// (colliding starts, run-time variation, shared blocks), plus
/// after-midnight starts.
#[derive(Clone, Debug)]
struct GenTrip {
    route: u8,
    service: u8,
    block: Option<u8>,
    /// Start as a 10-minute slot (0..150 → 00:00..25:00).
    start_slot: u16,
    /// Ordered distinct stop indices into s0..s5.
    stops: Vec<u8>,
    leg_min: u8,
}

fn gen_trip() -> impl Strategy<Value = GenTrip> {
    (
        0u8..2,
        0u8..2,
        proptest::option::of(0u8..2),
        0u16..150,
        proptest::collection::vec(0u8..6, 2..=4),
        1u8..30,
    )
        .prop_map(|(route, service, block, start_slot, stops, leg_min)| {
            let mut seq: Vec<u8> =
                stops.into_iter().collect::<std::collections::BTreeSet<_>>().into_iter().collect();
            if seq.len() < 2 {
                seq = vec![0, 1];
            }
            GenTrip { route, service, block, start_slot, stops: seq, leg_min }
        })
}

#[derive(Clone, Debug)]
struct SpecFeed {
    trips: Vec<GenTrip>,
    families: u16,
    density: u8,
    seed: u64,
}

fn gen_spec_feed() -> impl Strategy<Value = SpecFeed> {
    (
        // At least one trip: trips.txt and stop_times.txt are required.
        proptest::collection::vec(gen_trip(), 1..=6),
        0u16..ALL_FAMILIES,
        0u8..=100,
        any::<u64>(),
    )
        .prop_map(|(trips, families, density, seed)| SpecFeed { trips, families, density, seed })
}

/// splitmix64: the generator's value source, seeded per case.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn int(&mut self, lo: i64, hi: i64) -> String {
        (lo + self.below((hi - lo + 1) as u64) as i64).to_string()
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len() as u64) as usize]
    }
    /// Text with the characters CSV and UTF-8 handling get wrong.
    fn text(&mut self) -> String {
        let base = self.pick(&[
            "Main St", "Gare, Centrale", "The \"Express\"", "Café Nord", "Ünïcode ✓", "x",
            "Line\u{2028}Sep", "A/B & C",
        ]);
        format!("{base} {}", self.below(100))
    }
    fn url(&mut self) -> String {
        format!("https://example.org/{}", self.below(1000))
    }
    fn email(&mut self) -> String {
        format!("ops{}@example.org", self.below(100))
    }
    fn phone(&mut self) -> String {
        format!("+1-555-01{:02}", self.below(100))
    }
    fn color(&mut self) -> String {
        let c = format!("{:06x}", self.below(0x100_0000));
        if self.below(2) == 0 { c.to_ascii_uppercase() } else { c }
    }
    /// A coordinate written the ways real feeds write them: short
    /// decimals, trailing zeros, and full 15–17 digit doubles.
    fn coord(&mut self, base: f64) -> String {
        let x = base + (self.below(1_000_000) as f64) / 10_000_000.0;
        match self.below(3) {
            0 => format!("{x:.4}"),
            1 => format!("{x:.6}0"),
            _ => format!("{x}"),
        }
    }
    fn lat(&mut self) -> String {
        self.coord(45.5)
    }
    fn lon(&mut self) -> String {
        self.coord(-122.7)
    }
    /// HH:MM:SS, sometimes with the one-digit hour the spec allows.
    fn time(&mut self, secs: u32) -> String {
        let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
        if h < 10 && self.below(4) == 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{h:02}:{m:02}:{s:02}") }
    }
}

type Row = Vec<(&'static str, String)>;

struct Builder {
    rng: Rng,
    density: u8,
    tables: BTreeMap<&'static str, Vec<Row>>,
    geojson: Option<String>,
}

impl Builder {
    /// Whether an optional field is filled.
    fn opt(&mut self) -> bool {
        self.rng.below(100) < self.density as u64
    }
    /// Push an optional cell, valued by `value`, when `opt()` says so.
    fn maybe(&mut self, row: &mut Row, col: &'static str, value: impl FnOnce(&mut Rng) -> String) {
        if self.opt() {
            let v = value(&mut self.rng);
            row.push((col, v));
        }
    }
    fn push(&mut self, file: &'static str, row: Row) {
        self.tables.entry(file).or_default().push(row);
    }
}

fn build(feed: &SpecFeed) -> Builder {
    let has = |f: u16| feed.families & f != 0;
    let feed_info = has(FEED_INFO) || has(TRANSLATIONS);
    let mut b = Builder { rng: Rng(feed.seed), density: feed.density, tables: BTreeMap::new(), geojson: None };
    let s = |v: &str| v.to_string();
    let tz = b.rng.pick(&["America/Los_Angeles", "UTC", "Europe/Paris"]).to_string();

    // ---- agency.txt ----
    // agency_id is required with two agencies, optional with one; a
    // route names its agency only when the agency has an id.
    let two = has(TWO_AGENCIES);
    let mut agency_ids: Vec<Option<String>> = Vec::new();
    let mut agency_names: Vec<String> = Vec::new();
    for n in 1..=(if two { 2 } else { 1 }) {
        let id = if two || b.opt() { Some(n.to_string()) } else { None };
        let name = b.rng.text();
        let mut row: Row = vec![("agency_name", name.clone())];
        if let Some(id) = &id {
            row.push(("agency_id", id.clone()));
        }
        row.push(("agency_url", b.rng.url()));
        row.push(("agency_timezone", tz.clone()));
        b.maybe(&mut row, "agency_lang", |r| s(r.pick(&["en", "fr", "es-MX"])));
        b.maybe(&mut row, "agency_phone", Rng::phone);
        b.maybe(&mut row, "agency_fare_url", Rng::url);
        b.maybe(&mut row, "agency_email", Rng::email);
        b.maybe(&mut row, "cemv_support", |r| r.int(0, 2));
        b.push("agency.txt", row);
        agency_ids.push(id);
        agency_names.push(name);
    }
    let agency_of_route = |route: usize| -> Option<String> {
        agency_ids[if two && route == 1 { 1 } else { 0 }].clone()
    };

    // ---- levels.txt ----
    if has(STATIONS) {
        for (id, index) in [("L0", "0"), ("L1", "-1.5")] {
            let mut row: Row = vec![("level_id", s(id)), ("level_index", s(index))];
            b.maybe(&mut row, "level_name", Rng::text);
            b.push("levels.txt", row);
        }
    }

    // ---- stops.txt ----
    for i in 0..6usize {
        let mut row: Row = vec![
            ("stop_id", format!("s{i}")),
            ("stop_name", b.rng.text()),
            ("stop_lat", b.rng.lat()),
            ("stop_lon", b.rng.lon()),
        ];
        b.maybe(&mut row, "stop_code", |r| r.int(1000, 9999));
        b.maybe(&mut row, "tts_stop_name", Rng::text);
        b.maybe(&mut row, "stop_desc", Rng::text);
        b.maybe(&mut row, "stop_url", Rng::url);
        b.maybe(&mut row, "location_type", |_| s("0"));
        b.maybe(&mut row, "wheelchair_boarding", |r| r.int(0, 2));
        b.maybe(&mut row, "platform_code", |r| r.int(1, 9));
        if has(FARES_V1) && i < 2 {
            row.push(("zone_id", format!("z{i}")));
        } else {
            b.maybe(&mut row, "zone_id", |r| format!("z{}", r.below(2)));
        }
        if has(STATIONS) && i < 2 {
            row.push(("parent_station", s("st0")));
            b.maybe(&mut row, "level_id", |r| s(r.pick(&["L0", "L1"])));
            b.maybe(&mut row, "stop_access", |r| r.int(0, 1));
        } else {
            b.maybe(&mut row, "stop_timezone", |_| tz.clone());
        }
        b.push("stops.txt", row);
    }
    if has(STATIONS) {
        let mut station: Row = vec![
            ("stop_id", s("st0")), ("stop_name", b.rng.text()), ("location_type", s("1")),
            ("stop_lat", b.rng.lat()), ("stop_lon", b.rng.lon()),
        ];
        b.maybe(&mut station, "wheelchair_boarding", |r| r.int(0, 2));
        b.maybe(&mut station, "stop_timezone", |_| tz.clone());
        b.push("stops.txt", station);
        let mut entrance: Row = vec![
            ("stop_id", s("en0")), ("stop_name", b.rng.text()), ("location_type", s("2")),
            ("parent_station", s("st0")), ("stop_lat", b.rng.lat()), ("stop_lon", b.rng.lon()),
        ];
        b.maybe(&mut entrance, "level_id", |_| s("L0"));
        b.push("stops.txt", entrance);
        let mut node: Row = vec![("stop_id", s("nd0")), ("location_type", s("3")), ("parent_station", s("st0"))];
        b.maybe(&mut node, "stop_name", Rng::text);
        if b.opt() {
            node.push(("stop_lat", b.rng.lat()));
            node.push(("stop_lon", b.rng.lon()));
        }
        b.push("stops.txt", node);
        let mut boarding: Row = vec![("stop_id", s("ba0")), ("location_type", s("4")), ("parent_station", s("s0"))];
        b.maybe(&mut boarding, "stop_name", Rng::text);
        b.push("stops.txt", boarding);
    }

    // ---- networks: by routes.network_id, or by route_networks.txt
    // (the spec forbids both at once) ----
    let network_on_routes = has(FARES_V2) && b.rng.below(2) == 0;

    // ---- routes.txt ----
    let route_count = if has(FLEX) { 3 } else { 2 };
    for r in 0..route_count {
        let mut row: Row = vec![("route_id", r.to_string())];
        if let Some(a) = agency_of_route(r) {
            if two || b.opt() {
                row.push(("agency_id", a));
            }
        }
        // One of the two names is required.
        let short = b.opt();
        if short {
            row.push(("route_short_name", b.rng.int(1, 199)));
        }
        if !short || b.opt() {
            row.push(("route_long_name", b.rng.text()));
        }
        row.push(("route_type", s(b.rng.pick(&["0", "1", "2", "3", "4", "5", "6", "7", "11", "12"]))));
        b.maybe(&mut row, "route_desc", Rng::text);
        b.maybe(&mut row, "route_url", Rng::url);
        b.maybe(&mut row, "route_color", Rng::color);
        b.maybe(&mut row, "route_text_color", Rng::color);
        b.maybe(&mut row, "route_sort_order", |r| r.int(0, 100));
        // Continuous stopping is forbidden on a route whose trips
        // use pickup/drop-off windows (the flex route).
        if r != 2 {
            b.maybe(&mut row, "continuous_pickup", |r| r.int(0, 3));
            b.maybe(&mut row, "continuous_drop_off", |r| r.int(0, 3));
        }
        if network_on_routes && r < 2 {
            row.push(("network_id", s("n0")));
        }
        b.maybe(&mut row, "cemv_support", |r| r.int(0, 2));
        b.push("routes.txt", row);
    }

    // ---- shapes.txt ----
    if has(SHAPES) {
        for shape in ["sh0", "sh1"] {
            let points = 2 + b.rng.below(5);
            let mut seq = b.rng.below(2);
            let with_dist = b.opt();
            let mut dist = 0.0f64;
            for _ in 0..points {
                let mut row: Row = vec![
                    ("shape_id", s(shape)),
                    ("shape_pt_lat", b.rng.lat()),
                    ("shape_pt_lon", b.rng.lon()),
                    ("shape_pt_sequence", seq.to_string()),
                ];
                if with_dist {
                    row.push(("shape_dist_traveled", format!("{dist}")));
                    dist += (1 + b.rng.below(20_000)) as f64 / 7.0;
                }
                b.push("shapes.txt", row);
                seq += 1 + b.rng.below(5);
            }
        }
    }

    // ---- calendar.txt / calendar_dates.txt ----
    for (service, days) in [("0", [1, 1, 1, 1, 1, 0, 0]), ("1", [0, 0, 0, 0, 0, 1, 1])] {
        let names = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];
        let mut row: Row = vec![("service_id", s(service))];
        for (name, on) in names.iter().zip(days) {
            row.push((name, on.to_string()));
        }
        row.push(("start_date", s("20260101")));
        row.push(("end_date", s("20261231")));
        b.push("calendar.txt", row);
    }
    if has(CALENDAR_DATES) {
        for (service, date, kind) in [
            ("0", "20260703", "2"),
            ("1", "20260703", "1"),
            ("2", "20260801", "1"),
            ("2", "20260802", "1"),
        ] {
            b.push(
                "calendar_dates.txt",
                vec![("service_id", s(service)), ("date", s(date)), ("exception_type", s(kind))],
            );
        }
    }

    // ---- trips.txt + stop_times.txt (the scheduled core) ----
    // Blocks as the spec requires them: one vehicle, so a block's
    // trips on one service never overlap (a frequency trip — t0 under
    // FREQUENCIES — stays out of blocks). A block may span services.
    let mut spans: BTreeMap<(u8, u8), Vec<(u32, u32)>> = BTreeMap::new();
    let blocks: Vec<Option<u8>> = feed
        .trips
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let block = t.block.filter(|_| !(i == 0 && has(FREQUENCIES)))?;
            let start = t.start_slot as u32 * 600;
            let span = (start, start + (t.stops.len() as u32 - 1) * t.leg_min as u32 * 60 + 30);
            let taken = spans.entry((block, t.service)).or_default();
            if taken.iter().any(|&(a, z)| span.0 <= z && a <= span.1) {
                return None;
            }
            taken.push(span);
            Some(block)
        })
        .collect();
    let mut first_seq_of_t0 = None;
    for (i, t) in feed.trips.iter().enumerate() {
        let id = format!("t{i}");
        let mut row: Row = vec![
            ("route_id", t.route.to_string()),
            ("service_id", t.service.to_string()),
            ("trip_id", id.clone()),
        ];
        if let Some(block) = blocks[i] {
            row.push(("block_id", block.to_string()));
        }
        b.maybe(&mut row, "trip_headsign", Rng::text);
        b.maybe(&mut row, "trip_short_name", |r| r.int(100, 999));
        b.maybe(&mut row, "direction_id", |r| r.int(0, 1));
        if has(SHAPES) {
            b.maybe(&mut row, "shape_id", |r| format!("sh{}", r.below(2)));
        }
        b.maybe(&mut row, "wheelchair_accessible", |r| r.int(0, 2));
        b.maybe(&mut row, "bikes_allowed", |r| r.int(0, 2));
        b.maybe(&mut row, "cars_allowed", |r| r.int(0, 2));
        b.push("trips.txt", row);

        let start = t.start_slot as u32 * 600;
        let step = 1 + b.rng.below(3) as u32;
        let mut dist = 0.0f64;
        let last = t.stops.len() - 1;
        for (j, &stop) in t.stops.iter().enumerate() {
            let seq = 1 + j as u32 * step;
            if i == 0 && j == 0 {
                first_seq_of_t0 = Some(seq);
            }
            let arrival = start + j as u32 * t.leg_min as u32 * 60;
            let departure = arrival + if b.opt() { 30 } else { 0 };
            let mut row: Row = vec![
                ("trip_id", id.clone()),
                ("stop_id", format!("s{stop}")),
                ("stop_sequence", seq.to_string()),
            ];
            // An interior approximate row may leave its times empty;
            // the first and last rows always carry them.
            let timepoint = if b.opt() { Some(b.rng.below(2)) } else { None };
            let timeless = j != 0 && j != last && timepoint == Some(0) && b.opt();
            if !timeless {
                row.push(("arrival_time", b.rng.time(arrival)));
                row.push(("departure_time", b.rng.time(departure)));
            }
            if let Some(tp) = timepoint {
                row.push(("timepoint", tp.to_string()));
            }
            b.maybe(&mut row, "stop_headsign", Rng::text);
            b.maybe(&mut row, "pickup_type", |r| r.int(0, 3));
            b.maybe(&mut row, "drop_off_type", |r| r.int(0, 3));
            b.maybe(&mut row, "continuous_pickup", |r| r.int(0, 3));
            b.maybe(&mut row, "continuous_drop_off", |r| r.int(0, 3));
            if b.opt() {
                row.push(("shape_dist_traveled", format!("{dist}")));
            }
            dist += (1 + b.rng.below(5000)) as f64 / 3.0;
            b.push("stop_times.txt", row);
        }
    }

    // ---- frequencies.txt ----
    if has(FREQUENCIES) {
        if let Some(t) = feed.trips.first() {
            let start = t.start_slot as u32 * 600;
            let mut row: Row = vec![
                ("trip_id", s("t0")),
                ("start_time", b.rng.time(start)),
                ("end_time", b.rng.time(start + 7200)),
                ("headway_secs", b.rng.int(300, 3600)),
            ];
            b.maybe(&mut row, "exact_times", |r| r.int(0, 1));
            b.push("frequencies.txt", row);
        }
    }

    // ---- flex: location groups, locations.geojson, booking rules,
    // and a trip served by windows instead of times ----
    if has(FLEX) {
        let mut group: Row = vec![("location_group_id", s("lg0"))];
        b.maybe(&mut group, "location_group_name", Rng::text);
        b.push("location_groups.txt", group);
        for stop in ["s2", "s3"] {
            b.push("location_group_stops.txt", vec![("location_group_id", s("lg0")), ("stop_id", s(stop))]);
        }

        let mut properties = serde_json::Map::new();
        if b.opt() {
            properties.insert("stop_name".into(), b.rng.text().into());
        }
        if b.opt() {
            properties.insert("stop_desc".into(), b.rng.text().into());
        }
        let ring: Vec<serde_json::Value> = [(0.0, 0.0), (0.01, 0.0), (0.01, 0.01), (0.0, 0.0)]
            .iter()
            .map(|(dx, dy)| serde_json::json!([-122.7 + dx, 45.5 + dy]))
            .collect();
        b.geojson = Some(
            serde_json::json!({
                "type": "FeatureCollection",
                "features": [{
                    "type": "Feature",
                    "id": "loc0",
                    "properties": properties,
                    "geometry": { "type": "Polygon", "coordinates": [ring] },
                }],
            })
            .to_string(),
        );

        let booking_type = b.rng.below(3);
        let mut rule: Row = vec![("booking_rule_id", s("br0")), ("booking_type", booking_type.to_string())];
        match booking_type {
            1 => {
                rule.push(("prior_notice_duration_min", b.rng.int(15, 120)));
                b.maybe(&mut rule, "prior_notice_duration_max", |r| r.int(200, 2000));
            }
            2 => {
                rule.push(("prior_notice_last_day", b.rng.int(1, 3)));
                rule.push(("prior_notice_last_time", b.rng.time(17 * 3600)));
                if b.opt() {
                    rule.push(("prior_notice_start_day", b.rng.int(4, 14)));
                    rule.push(("prior_notice_start_time", b.rng.time(8 * 3600)));
                }
                b.maybe(&mut rule, "prior_notice_service_id", |_| s("0"));
            }
            _ => {}
        }
        b.maybe(&mut rule, "message", Rng::text);
        b.maybe(&mut rule, "pickup_message", Rng::text);
        b.maybe(&mut rule, "drop_off_message", Rng::text);
        b.maybe(&mut rule, "phone_number", Rng::phone);
        b.maybe(&mut rule, "info_url", Rng::url);
        b.maybe(&mut rule, "booking_url", Rng::url);
        b.push("booking_rules.txt", rule);

        let mut trip: Row = vec![("route_id", s("2")), ("service_id", s("0")), ("trip_id", s("f0"))];
        b.maybe(&mut trip, "trip_headsign", Rng::text);
        b.push("trips.txt", trip);
        let open = b.rng.time(8 * 3600);
        let close = b.rng.time(12 * 3600);
        b.push("stop_times.txt", vec![
            ("trip_id", s("f0")), ("location_group_id", s("lg0")), ("stop_sequence", s("1")),
            ("start_pickup_drop_off_window", open.clone()), ("end_pickup_drop_off_window", close.clone()),
            ("pickup_type", s("2")), ("drop_off_type", s("1")), ("pickup_booking_rule_id", s("br0")),
        ]);
        b.push("stop_times.txt", vec![
            ("trip_id", s("f0")), ("location_id", s("loc0")), ("stop_sequence", s("2")),
            ("start_pickup_drop_off_window", open), ("end_pickup_drop_off_window", close),
            ("pickup_type", s("1")), ("drop_off_type", s("2")), ("drop_off_booking_rule_id", s("br0")),
        ]);
    }

    // ---- transfers.txt ----
    if has(TRANSFERS) {
        let kind = b.rng.below(4);
        let mut row: Row = vec![("from_stop_id", s("s0")), ("to_stop_id", s("s1")), ("transfer_type", kind.to_string())];
        if kind == 2 {
            row.push(("min_transfer_time", b.rng.int(60, 600)));
        }
        b.push("transfers.txt", row);
        b.push("transfers.txt", vec![
            ("from_stop_id", s("s2")), ("to_stop_id", s("s3")), ("from_route_id", s("0")),
            ("to_route_id", s("1")), ("transfer_type", s("1")),
        ]);
        if feed.trips.len() >= 2 {
            let kind = b.rng.int(4, 5);
            b.push("transfers.txt", vec![
                ("from_trip_id", s("t0")), ("to_trip_id", s("t1")), ("transfer_type", kind),
            ]);
        }
    }

    // ---- pathways.txt ----
    if has(STATIONS) {
        let mut walkway: Row = vec![
            ("pathway_id", s("p0")), ("from_stop_id", s("en0")), ("to_stop_id", s("nd0")),
            ("pathway_mode", s("1")), ("is_bidirectional", s("1")),
        ];
        b.maybe(&mut walkway, "length", |r| format!("{}", r.below(5000) as f64 / 10.0));
        b.maybe(&mut walkway, "traversal_time", |r| r.int(5, 300));
        b.maybe(&mut walkway, "max_slope", |r| format!("{}", r.below(100) as f64 / 1000.0));
        b.maybe(&mut walkway, "min_width", |r| format!("{}", 1.0 + r.below(30) as f64 / 10.0));
        b.maybe(&mut walkway, "signposted_as", Rng::text);
        b.maybe(&mut walkway, "reversed_signposted_as", Rng::text);
        b.push("pathways.txt", walkway);
        let mut stairs: Row = vec![
            ("pathway_id", s("p1")), ("from_stop_id", s("nd0")), ("to_stop_id", s("s0")),
            ("pathway_mode", s("2")), ("is_bidirectional", b.rng.int(0, 1)),
        ];
        b.maybe(&mut stairs, "stair_count", |r| r.int(-40, 40));
        b.maybe(&mut stairs, "traversal_time", |r| r.int(5, 300));
        b.push("pathways.txt", stairs);
        // An escalator is one-way by the spec.
        b.push("pathways.txt", vec![
            ("pathway_id", s("p2")), ("from_stop_id", s("nd0")), ("to_stop_id", s("ba0")),
            ("pathway_mode", s("4")), ("is_bidirectional", s("0")),
        ]);
    }

    // ---- fares v1 ----
    if has(FARES_V1) {
        for (fare, price) in [("fa0", "2.50"), ("fa1", "0")] {
            let mut row: Row = vec![
                ("fare_id", s(fare)),
                ("price", s(price)),
                ("currency_type", s(b.rng.pick(&["USD", "CAD"]))),
                ("payment_method", b.rng.int(0, 1)),
                // Required column; empty means unlimited transfers.
                ("transfers", s(b.rng.pick(&["", "0", "1", "2"]))),
            ];
            if let Some(a) = &agency_ids[0] {
                if two || b.opt() {
                    row.push(("agency_id", a.clone()));
                }
            }
            b.maybe(&mut row, "transfer_duration", |r| r.int(600, 7200));
            b.push("fare_attributes.txt", row);
        }
        b.push("fare_rules.txt", vec![("fare_id", s("fa0")), ("route_id", s("0"))]);
        b.push("fare_rules.txt", vec![("fare_id", s("fa0")), ("origin_id", s("z0")), ("destination_id", s("z1"))]);
        b.push("fare_rules.txt", vec![("fare_id", s("fa1")), ("contains_id", s("z0"))]);
    }

    // ---- fares v2 ----
    if has(FARES_V2) {
        let mut network: Row = vec![("network_id", s("n0"))];
        b.maybe(&mut network, "network_name", Rng::text);
        b.push("networks.txt", network);
        if !network_on_routes {
            for route in ["0", "1"] {
                b.push("route_networks.txt", vec![("network_id", s("n0")), ("route_id", s(route))]);
            }
        }
        for (area, stop) in [("a0", "s0"), ("a1", "s1")] {
            let mut row: Row = vec![("area_id", s(area))];
            b.maybe(&mut row, "area_name", Rng::text);
            b.push("areas.txt", row);
            b.push("stop_areas.txt", vec![("area_id", s(area)), ("stop_id", s(stop))]);
        }
        let mut peak: Row = vec![("timeframe_group_id", s("tf0")), ("service_id", s("0"))];
        if b.opt() {
            peak.push(("start_time", b.rng.time(7 * 3600)));
            peak.push(("end_time", b.rng.time(9 * 3600)));
        }
        b.push("timeframes.txt", peak);
        b.push("timeframes.txt", vec![("timeframe_group_id", s("tf0")), ("service_id", s("1"))]);
        for (category, default) in [("rc0", "1"), ("rc1", "0")] {
            let mut row: Row = vec![("rider_category_id", s(category)), ("rider_category_name", b.rng.text())];
            b.maybe(&mut row, "is_default_fare_category", |_| s(default));
            b.maybe(&mut row, "eligibility_url", Rng::url);
            b.push("rider_categories.txt", row);
        }
        let mut cash: Row = vec![("fare_media_id", s("fm0")), ("fare_media_type", s("0"))];
        b.maybe(&mut cash, "fare_media_name", Rng::text);
        b.push("fare_media.txt", cash);
        let (name, kind) = (b.rng.text(), b.rng.int(1, 4));
        b.push("fare_media.txt", vec![
            ("fare_media_id", s("fm1")), ("fare_media_name", name), ("fare_media_type", kind),
        ]);
        let mut fare: Row = vec![("fare_product_id", s("fp0")), ("amount", s("2.75")), ("currency", s("USD"))];
        b.maybe(&mut fare, "fare_product_name", Rng::text);
        b.maybe(&mut fare, "rider_category_id", |_| s("rc0"));
        b.maybe(&mut fare, "fare_media_id", |_| s("fm1"));
        b.push("fare_products.txt", fare);
        b.push("fare_products.txt", vec![("fare_product_id", s("fp1")), ("amount", s("0.00")), ("currency", s("USD"))]);
        let mut leg: Row = vec![
            ("leg_group_id", s("lgA")), ("network_id", s("n0")), ("from_area_id", s("a0")),
            ("to_area_id", s("a1")), ("fare_product_id", s("fp0")),
        ];
        b.maybe(&mut leg, "from_timeframe_group_id", |_| s("tf0"));
        b.maybe(&mut leg, "to_timeframe_group_id", |_| s("tf0"));
        b.maybe(&mut leg, "rule_priority", |r| r.int(0, 3));
        b.push("fare_leg_rules.txt", leg);
        let mut join: Row = vec![("from_network_id", s("n0")), ("to_network_id", s("n0"))];
        if b.opt() {
            join.push(("from_stop_id", s("s1")));
            join.push(("to_stop_id", s("s1")));
        }
        b.push("fare_leg_join_rules.txt", join);
        // From and to the same leg group: transfer_count is required.
        let mut transfer: Row = vec![
            ("from_leg_group_id", s("lgA")), ("to_leg_group_id", s("lgA")),
            ("transfer_count", s(b.rng.pick(&["-1", "1", "2"]))),
            ("fare_transfer_type", b.rng.int(0, 2)),
        ];
        if b.opt() {
            transfer.push(("duration_limit", b.rng.int(600, 7200)));
            transfer.push(("duration_limit_type", b.rng.int(0, 3)));
        }
        b.maybe(&mut transfer, "fare_product_id", |_| s("fp1"));
        b.push("fare_transfer_rules.txt", transfer);
    }

    // ---- translations.txt ----
    if has(TRANSLATIONS) {
        let text = b.rng.text();
        b.push("translations.txt", vec![
            ("table_name", s("stops")), ("field_name", s("stop_name")), ("language", s("fr")),
            ("translation", text), ("record_id", s("s0")),
        ]);
        let mut agency: Row = vec![
            ("table_name", s("agency")), ("field_name", s("agency_name")), ("language", s("es")),
            ("translation", b.rng.text()),
        ];
        match &agency_ids[0] {
            Some(id) => agency.push(("record_id", id.clone())),
            None => agency.push(("field_value", agency_names[0].clone())),
        }
        b.push("translations.txt", agency);
        let text = b.rng.text();
        b.push("translations.txt", vec![
            ("table_name", s("feed_info")), ("field_name", s("feed_publisher_name")),
            ("language", s("fr")), ("translation", text),
        ]);
        if let Some(seq) = first_seq_of_t0 {
            let text = b.rng.text();
            b.push("translations.txt", vec![
                ("table_name", s("stop_times")), ("field_name", s("stop_headsign")),
                ("language", s("fr")), ("translation", text),
                ("record_id", s("t0")), ("record_sub_id", seq.to_string()),
            ]);
        }
    }

    // ---- feed_info.txt ----
    if feed_info {
        let mut row: Row = vec![
            ("feed_publisher_name", b.rng.text()),
            ("feed_publisher_url", b.rng.url()),
            ("feed_lang", s(b.rng.pick(&["en", "fr", "mul"]))),
        ];
        b.maybe(&mut row, "default_lang", |_| s("en"));
        if b.opt() {
            row.push(("feed_start_date", s("20260101")));
            row.push(("feed_end_date", s("20261231")));
        }
        b.maybe(&mut row, "feed_version", |r| format!("v{}", r.below(1000)));
        b.maybe(&mut row, "feed_contact_email", Rng::email);
        b.maybe(&mut row, "feed_contact_url", Rng::url);
        b.push("feed_info.txt", row);
    }

    // ---- attributions.txt ----
    if has(ATTRIBUTIONS) {
        let mut row: Row = vec![("organization_name", b.rng.text()), ("is_producer", s("1"))];
        b.maybe(&mut row, "attribution_id", |_| s("attr0"));
        b.maybe(&mut row, "is_operator", |r| r.int(0, 1));
        b.maybe(&mut row, "is_authority", |r| r.int(0, 1));
        // At most one of agency_id, route_id, trip_id.
        match b.rng.below(4) {
            0 => row.push(("route_id", s("0"))),
            1 if !feed.trips.is_empty() => row.push(("trip_id", s("t0"))),
            2 => {
                if let Some(a) = &agency_ids[0] {
                    row.push(("agency_id", a.clone()));
                }
            }
            _ => {}
        }
        b.maybe(&mut row, "attribution_url", Rng::url);
        b.maybe(&mut row, "attribution_email", Rng::email);
        b.maybe(&mut row, "attribution_phone", Rng::phone);
        b.push("attributions.txt", row);
    }

    b
}

/// The generated feed as zip bytes. Each file's header is the
/// union of its rows' columns in a seeded order (column order is
/// not significant), and every column is checked against the spec
/// table — a typo in the generator fails here, not as a fake loss.
fn zip_feed(feed: &SpecFeed) -> Vec<u8> {
    let mut built = build(feed);
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let tables = std::mem::take(&mut built.tables);
    for (file, rows) in tables {
        let spec = spec(file);
        let mut header: Vec<&'static str> = spec
            .columns
            .iter()
            .map(|(c, _)| *c)
            .filter(|c| rows.iter().any(|r| r.iter().any(|(k, _)| k == c)))
            .collect();
        for row in &rows {
            for (k, _) in row {
                assert!(spec.columns.iter().any(|(c, _)| c == k), "generator wrote {k} into {file}");
            }
        }
        for i in (1..header.len()).rev() {
            let j = built.rng.below(i as u64 + 1) as usize;
            header.swap(i, j);
        }
        let mut csv = csv::Writer::from_writer(Vec::new());
        csv.write_record(&header).unwrap();
        for row in &rows {
            let cells: Vec<&str> = header
                .iter()
                .map(|c| row.iter().find(|(k, _)| k == c).map_or("", |(_, v)| v.as_str()))
                .collect();
            csv.write_record(&cells).unwrap();
        }
        w.start_file(file, opts).unwrap();
        w.write_all(&csv.into_inner().unwrap()).unwrap();
    }
    if let Some(geojson) = &built.geojson {
        w.start_file(LOCATIONS, opts).unwrap();
        w.write_all(geojson.as_bytes()).unwrap();
    }
    w.finish().unwrap().into_inner()
}

// ===================================================================
// The oracle: raw tables, typed by the spec table.
// ===================================================================

/// A row: column → normalized value, empty cells omitted.
type Normalized = BTreeMap<String, String>;

fn members(zip_bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes)).map_err(|e| format!("not a zip: {e}"))?;
    let mut out = BTreeMap::new();
    for i in 0..archive.len() {
        let mut member = archive.by_index(i).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        member.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        out.insert(member.name().to_string(), bytes);
    }
    Ok(out)
}

fn seconds(text: &str) -> Option<u32> {
    let mut parts = text.split(':');
    let (h, m, s) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || m.len() != 2 || s.len() != 2 {
        return None;
    }
    let (h, m, s): (u32, u32, u32) = (h.parse().ok()?, m.parse().ok()?, s.parse().ok()?);
    (m < 60 && s < 60).then_some(h * 3600 + m * 60 + s)
}

fn normalize(kind: Kind, raw: &str) -> String {
    let v = raw.trim();
    let typed = match kind {
        Text => Some(raw.to_string()),
        Int => v.parse::<i64>().ok().map(|n| n.to_string()),
        Float => v.parse::<f64>().ok().map(|x| format!("{x}")),
        Time => seconds(v).map(|s| format!("{s}s")),
        Color => Some(v.to_ascii_uppercase()),
    };
    typed.unwrap_or_else(|| format!("<unparseable {raw:?}>"))
}

fn table(file: &FileSpec, bytes: Option<&Vec<u8>>) -> Result<Vec<Normalized>, String> {
    let Some(bytes) = bytes else { return Ok(Vec::new()) };
    let text = std::str::from_utf8(bytes).map_err(|e| format!("{}: not UTF-8: {e}", file.name))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut reader = csv::ReaderBuilder::new().flexible(true).from_reader(text.as_bytes());
    let header: Vec<String> = reader
        .headers()
        .map_err(|e| format!("{}: {e}", file.name))?
        .iter()
        .map(|h| h.trim().to_string())
        .collect();
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| format!("{}: {e}", file.name))?;
        let row: Normalized = header
            .iter()
            .zip(record.iter())
            .filter(|(_, v)| !v.is_empty())
            .map(|(h, v)| {
                // A column outside the spec compares as text: an
                // invented column is a difference, not noise.
                let kind = file.columns.iter().find(|(c, _)| *c == h.as_str()).map_or(Text, |(_, k)| *k);
                (h.clone(), normalize(kind, v))
            })
            .filter(|(h, v)| {
                !SPEC_DEFAULTS.iter().any(|(f, c, default)| {
                    *f == file.name && c == h && *v == normalize(Int, default)
                })
            })
            .collect();
        if !row.is_empty() {
            rows.push(row);
        }
    }
    rows.sort();
    Ok(rows)
}

/// Rows in `a` and not in `b`, as multisets.
fn missing<'x>(a: &'x [Normalized], b: &'x [Normalized]) -> Vec<Normalized> {
    let mut counts: BTreeMap<&'x Normalized, i64> = BTreeMap::new();
    for row in b {
        *counts.entry(row).or_default() += 1;
    }
    a.iter()
        .filter(|row| {
            let c = counts.entry(*row).or_default();
            *c -= 1;
            *c < 0
        })
        .cloned()
        .collect()
}

fn show(rows: &[Normalized]) -> String {
    let shown: Vec<String> = rows.iter().take(4).map(|r| format!("  {r:?}")).collect();
    let more = rows.len().saturating_sub(4);
    format!("{}{}", shown.join("\n"), if more > 0 { format!("\n  … {more} more") } else { String::new() })
}

/// The law for one file: the source feed's rows are exactly the
/// round-tripped feed's rows.
fn file_survives(feed: &SpecFeed, file: &str) -> Result<(), TestCaseError> {
    file_survives_through(feed, file, round_trip)
}

fn file_survives_through(
    feed: &SpecFeed,
    file: &str,
    round_trip: fn(&[u8]) -> Result<Vec<u8>, String>,
) -> Result<(), TestCaseError> {
    let source = zip_feed(feed);
    let out = round_trip(&source).map_err(TestCaseError::fail)?;
    let (before, after) = (
        members(&source).map_err(TestCaseError::fail)?,
        members(&out).map_err(TestCaseError::fail)?,
    );
    if file == LOCATIONS {
        let parse = |bytes: Option<&Vec<u8>>| -> Result<Option<serde_json::Value>, TestCaseError> {
            bytes
                .map(|b| serde_json::from_slice::<serde_json::Value>(b).map_err(|e| TestCaseError::fail(format!("{LOCATIONS}: {e}"))))
                .transpose()
        };
        let (a, b) = (parse(before.get(LOCATIONS))?, parse(after.get(LOCATIONS))?);
        prop_assert_eq!(a, b, "{} did not survive the round trip", LOCATIONS);
        return Ok(());
    }
    let spec = spec(file);
    let a = table(spec, before.get(file)).map_err(TestCaseError::fail)?;
    let b = table(spec, after.get(file)).map_err(TestCaseError::fail)?;
    if a != b {
        let (lost, invented) = (missing(&a, &b), missing(&b, &a));
        return Err(TestCaseError::fail(format!(
            "{file} did not survive the round trip: {} rows in, {} out\nlost ({}):\n{}\ninvented ({}):\n{}",
            a.len(),
            b.len(),
            lost.len(),
            show(&lost),
            invented.len(),
            show(&invented),
        )));
    }
    Ok(())
}

// ===================================================================
// The laws: one per file of the spec.
// ===================================================================

macro_rules! every_file_survives {
    ($($law:ident => $file:expr),* $(,)?) => {
        proptest! {
            #![proptest_config(ProptestConfig::with_cases(64))]
            $(
                #[test]
                fn $law(feed in gen_spec_feed()) {
                    file_survives(&feed, $file)?;
                }
            )*
        }

        /// The law list covers the spec table, and nothing else.
        #[test]
        fn every_spec_file_has_a_law() {
            let mut laws: Vec<&str> = vec![$($file),*];
            laws.retain(|f| *f != LOCATIONS);
            laws.sort();
            let mut spec: Vec<&str> = SPEC.iter().map(|f| f.name).collect();
            spec.sort();
            assert_eq!(laws, spec);
        }
    };
}

every_file_survives! {
    agency_survives => "agency.txt",
    stops_survive => "stops.txt",
    routes_survive => "routes.txt",
    trips_survive => "trips.txt",
    stop_times_survive => "stop_times.txt",
    calendar_survives => "calendar.txt",
    calendar_dates_survive => "calendar_dates.txt",
    fare_attributes_survive => "fare_attributes.txt",
    fare_rules_survive => "fare_rules.txt",
    timeframes_survive => "timeframes.txt",
    rider_categories_survive => "rider_categories.txt",
    fare_media_survive => "fare_media.txt",
    fare_products_survive => "fare_products.txt",
    fare_leg_rules_survive => "fare_leg_rules.txt",
    fare_leg_join_rules_survive => "fare_leg_join_rules.txt",
    fare_transfer_rules_survive => "fare_transfer_rules.txt",
    areas_survive => "areas.txt",
    stop_areas_survive => "stop_areas.txt",
    networks_survive => "networks.txt",
    route_networks_survive => "route_networks.txt",
    shapes_survive => "shapes.txt",
    frequencies_survive => "frequencies.txt",
    transfers_survive => "transfers.txt",
    pathways_survive => "pathways.txt",
    levels_survive => "levels.txt",
    location_groups_survive => "location_groups.txt",
    location_group_stops_survive => "location_group_stops.txt",
    locations_survive => LOCATIONS,
    booking_rules_survive => "booking_rules.txt",
    translations_survive => "translations.txt",
    feed_info_survives => "feed_info.txt",
    attributions_survive => "attributions.txt",
}

/// The oracle itself: a feed compared with itself is the same feed,
/// and the generator's output is readable as raw tables — so a
/// failing law above is the codec's loss, never the oracle's.
#[test]
fn the_oracle_accepts_the_identity() {
    let trip = |route, start_slot| GenTrip {
        route,
        service: 0,
        block: Some(0),
        start_slot,
        stops: vec![0, 2, 3, 5],
        leg_min: 7,
    };
    let feed = SpecFeed { trips: vec![trip(0, 40), trip(1, 50)], families: ALL_FAMILIES - 1, density: 100, seed: 7 };
    let bytes = zip_feed(&feed);
    let files = members(&bytes).unwrap();
    assert!(files.contains_key(LOCATIONS), "the full-family feed writes locations.geojson");
    for spec in SPEC {
        assert!(files.contains_key(spec.name), "the full-family feed writes {}", spec.name);
        let rows = table(spec, files.get(spec.name)).unwrap();
        assert_eq!(rows, table(spec, files.get(spec.name)).unwrap());
        assert!(
            rows.iter().all(|r| r.values().all(|v| !v.starts_with("<unparseable"))),
            "{} wrote a value its own type cannot read",
            spec.name
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// The codec's serialization layer alone reads and writes every
    /// spec file without loss — so a failing law above is a loss in
    /// the network import or export, never in the feed layer.
    #[test]
    fn the_feed_layer_is_lossless(feed in gen_spec_feed()) {
        let through_feed = |bytes: &[u8]| -> Result<Vec<u8>, String> {
            Ok(transit_codec::Feed::from_zip_bytes(bytes).map_err(|e| e.to_string())?.to_zip_bytes())
        };
        for file in SPEC.iter().map(|f| f.name).chain([LOCATIONS]) {
            file_survives_through(&feed, file, through_feed)?;
        }
    }
}
