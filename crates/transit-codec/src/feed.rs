//! The GTFS Schedule feed, whole: every file and every column of the
//! spec, read and written without loss.
//!
//! This is the boundary's serialization layer and nothing more. Each
//! file is a list of rows; each row keeps every spec column as the
//! exact text the feed carried (`None` = empty or absent). Typing is
//! the import's job, where the model needs it — keeping text here is
//! what makes `Feed::from_zip_bytes` → `Feed::to_zip_bytes` lossless
//! by construction. Extension files and columns outside the spec are
//! not read.
//!
//! Writing is deterministic: files in spec order, columns in spec
//! order (a column appears when the spec requires it or any row fills
//! it), rows in the order given, fixed zip metadata — so the zip's
//! content hash is a document identity (the capability-bus rule).

use std::io::{Cursor, Read, Write};

use serde::{Deserialize, Deserializer};

/// Why a feed could not be read.
#[derive(Debug, Clone, PartialEq)]
pub enum FeedError {
    Zip(String),
    MissingFile(&'static str),
    Csv { file: &'static str, message: String },
    Json { file: &'static str, message: String },
}

impl std::fmt::Display for FeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FeedError::Zip(m) => write!(f, "not a readable zip: {m}"),
            FeedError::MissingFile(name) => write!(f, "missing required file: {name}"),
            FeedError::Csv { file, message } => write!(f, "{file}: {message}"),
            FeedError::Json { file, message } => write!(f, "{file}: {message}"),
        }
    }
}

impl std::error::Error for FeedError {}

/// One spec file's row type.
pub trait Table: for<'de> Deserialize<'de> {
    /// The file name inside the zip.
    const FILE: &'static str;
    /// Every spec column, in spec order.
    const COLUMNS: &'static [&'static str];
    /// Columns the spec requires in the header even when every row
    /// leaves them empty (an empty `fare_attributes.transfers` means
    /// unlimited, so the column must exist).
    const REQUIRED: &'static [&'static str];
    /// The row's cells, in `COLUMNS` order.
    fn cells(&self) -> Vec<Option<&str>>;
}

/// An empty cell is an absent value.
fn empty_as_none<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<String>::deserialize(d)?.filter(|s| !s.is_empty()))
}

macro_rules! gtfs_files {
    ($(
        $(#[$doc:meta])*
        $field:ident: $row:ident = $file:literal
            required [$($req:ident),* $(,)?]
            [$($col:ident),* $(,)?];
    )*) => {
        $(
            $(#[$doc])*
            #[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
            pub struct $row {
                $(
                    #[serde(default, deserialize_with = "empty_as_none")]
                    pub $col: Option<String>,
                )*
            }

            impl Table for $row {
                const FILE: &'static str = $file;
                const COLUMNS: &'static [&'static str] = &[$(stringify!($col)),*];
                const REQUIRED: &'static [&'static str] = &[$(stringify!($req)),*];
                fn cells(&self) -> Vec<Option<&str>> {
                    vec![$(self.$col.as_deref()),*]
                }
            }
        )*

        /// A whole GTFS Schedule feed: one row list per spec file,
        /// plus `locations.geojson`.
        #[derive(Debug, Clone, Default, PartialEq)]
        pub struct Feed {
            $(pub $field: Vec<$row>,)*
            /// `locations.geojson`, as the JSON value it is.
            pub locations: Option<serde_json::Value>,
        }

        impl Feed {
            /// Read every spec file the zip carries.
            pub fn from_zip_bytes(bytes: &[u8]) -> Result<Feed, FeedError> {
                let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
                    .map_err(|e| FeedError::Zip(e.to_string()))?;
                let mut feed = Feed::default();
                $(feed.$field = read_table::<$row>(&mut archive)?;)*
                if let Some(text) = read_member(&mut archive, LOCATIONS)? {
                    feed.locations = Some(serde_json::from_str(&text).map_err(|e| FeedError::Json {
                        file: LOCATIONS,
                        message: e.to_string(),
                    })?);
                }
                for required in ["agency.txt", "routes.txt", "trips.txt", "stop_times.txt"] {
                    if !has_member(&mut archive, required) {
                        return Err(FeedError::MissingFile(required));
                    }
                }
                // stops.txt is conditionally required: a feed whose
                // service is all zones (locations.geojson) may omit it.
                if !has_member(&mut archive, "stops.txt") && feed.locations.is_none() {
                    return Err(FeedError::MissingFile("stops.txt"));
                }
                if !has_member(&mut archive, "calendar.txt") && !has_member(&mut archive, "calendar_dates.txt") {
                    return Err(FeedError::MissingFile("calendar.txt"));
                }
                Ok(feed)
            }

            /// Write the feed as zip bytes. Deterministic: the same
            /// feed yields the same bytes on every host.
            pub fn to_zip_bytes(&self) -> Vec<u8> {
                let mut zip = ZipWriter::new();
                $(
                    if !self.$field.is_empty() || ALWAYS_WRITTEN.contains(&$file) {
                        zip.add($file, &render(&self.$field));
                    }
                )*
                if let Some(locations) = &self.locations {
                    zip.add(LOCATIONS, locations.to_string().as_bytes());
                }
                zip.finish()
            }
        }
    };
}

const LOCATIONS: &str = "locations.geojson";

/// Written even when empty: the spec requires these files, and the
/// calendar needs one of its two (a network with no service still
/// publishes a parseable feed).
const ALWAYS_WRITTEN: &[&str] =
    &["agency.txt", "stops.txt", "routes.txt", "trips.txt", "stop_times.txt", "calendar.txt"];

gtfs_files! {
    agency: AgencyRow = "agency.txt" required [agency_name, agency_url, agency_timezone] [
        agency_id, agency_name, agency_url, agency_timezone, agency_lang, agency_phone,
        agency_fare_url, agency_email, cemv_support,
    ];
    stops: StopRow = "stops.txt" required [stop_id] [
        stop_id, stop_code, stop_name, tts_stop_name, stop_desc, stop_lat, stop_lon, zone_id,
        stop_url, location_type, parent_station, stop_timezone, wheelchair_boarding, level_id,
        platform_code, stop_access,
    ];
    routes: RouteRow = "routes.txt" required [route_id, route_type] [
        route_id, agency_id, route_short_name, route_long_name, route_desc, route_type, route_url,
        route_color, route_text_color, route_sort_order, continuous_pickup, continuous_drop_off,
        network_id, cemv_support,
    ];
    trips: TripRow = "trips.txt" required [route_id, service_id, trip_id] [
        route_id, service_id, trip_id, trip_headsign, trip_short_name, direction_id, block_id,
        shape_id, wheelchair_accessible, bikes_allowed, cars_allowed,
    ];
    stop_times: StopTimeRow = "stop_times.txt" required [trip_id, stop_sequence] [
        trip_id, arrival_time, departure_time, stop_id, location_group_id, location_id,
        stop_sequence, stop_headsign, start_pickup_drop_off_window, end_pickup_drop_off_window,
        pickup_type, drop_off_type, continuous_pickup, continuous_drop_off, shape_dist_traveled,
        timepoint, pickup_booking_rule_id, drop_off_booking_rule_id,
    ];
    calendar: CalendarRow = "calendar.txt" required [
        service_id, monday, tuesday, wednesday, thursday, friday, saturday, sunday, start_date,
        end_date,
    ] [
        service_id, monday, tuesday, wednesday, thursday, friday, saturday, sunday, start_date,
        end_date,
    ];
    calendar_dates: CalendarDateRow = "calendar_dates.txt" required [service_id, date, exception_type] [
        service_id, date, exception_type,
    ];
    fare_attributes: FareAttributeRow = "fare_attributes.txt" required [
        fare_id, price, currency_type, payment_method, transfers,
    ] [
        fare_id, price, currency_type, payment_method, transfers, agency_id, transfer_duration,
    ];
    fare_rules: FareRuleRow = "fare_rules.txt" required [fare_id] [
        fare_id, route_id, origin_id, destination_id, contains_id,
    ];
    timeframes: TimeframeRow = "timeframes.txt" required [timeframe_group_id, service_id] [
        timeframe_group_id, start_time, end_time, service_id,
    ];
    rider_categories: RiderCategoryRow = "rider_categories.txt" required [
        rider_category_id, rider_category_name,
    ] [
        rider_category_id, rider_category_name, is_default_fare_category, eligibility_url,
    ];
    fare_media: FareMediaRow = "fare_media.txt" required [fare_media_id, fare_media_type] [
        fare_media_id, fare_media_name, fare_media_type,
    ];
    fare_products: FareProductRow = "fare_products.txt" required [fare_product_id, amount, currency] [
        fare_product_id, fare_product_name, rider_category_id, fare_media_id, amount, currency,
    ];
    fare_leg_rules: FareLegRuleRow = "fare_leg_rules.txt" required [fare_product_id] [
        leg_group_id, network_id, from_area_id, to_area_id, from_timeframe_group_id,
        to_timeframe_group_id, fare_product_id, rule_priority,
    ];
    fare_leg_join_rules: FareLegJoinRuleRow = "fare_leg_join_rules.txt" required [
        from_network_id, to_network_id,
    ] [
        from_network_id, to_network_id, from_stop_id, to_stop_id,
    ];
    fare_transfer_rules: FareTransferRuleRow = "fare_transfer_rules.txt" required [fare_transfer_type] [
        from_leg_group_id, to_leg_group_id, transfer_count, duration_limit, duration_limit_type,
        fare_transfer_type, fare_product_id,
    ];
    areas: AreaRow = "areas.txt" required [area_id] [area_id, area_name];
    stop_areas: StopAreaRow = "stop_areas.txt" required [area_id, stop_id] [area_id, stop_id];
    networks: NetworkRow = "networks.txt" required [network_id] [network_id, network_name];
    route_networks: RouteNetworkRow = "route_networks.txt" required [network_id, route_id] [
        network_id, route_id,
    ];
    shapes: ShapeRow = "shapes.txt" required [shape_id, shape_pt_lat, shape_pt_lon, shape_pt_sequence] [
        shape_id, shape_pt_lat, shape_pt_lon, shape_pt_sequence, shape_dist_traveled,
    ];
    frequencies: FrequencyRow = "frequencies.txt" required [trip_id, start_time, end_time, headway_secs] [
        trip_id, start_time, end_time, headway_secs, exact_times,
    ];
    transfers: TransferRow = "transfers.txt" required [transfer_type] [
        from_stop_id, to_stop_id, from_route_id, to_route_id, from_trip_id, to_trip_id,
        transfer_type, min_transfer_time,
    ];
    pathways: PathwayRow = "pathways.txt" required [
        pathway_id, from_stop_id, to_stop_id, pathway_mode, is_bidirectional,
    ] [
        pathway_id, from_stop_id, to_stop_id, pathway_mode, is_bidirectional, length,
        traversal_time, stair_count, max_slope, min_width, signposted_as, reversed_signposted_as,
    ];
    levels: LevelRow = "levels.txt" required [level_id, level_index] [level_id, level_index, level_name];
    location_groups: LocationGroupRow = "location_groups.txt" required [location_group_id] [
        location_group_id, location_group_name,
    ];
    location_group_stops: LocationGroupStopRow = "location_group_stops.txt" required [
        location_group_id, stop_id,
    ] [
        location_group_id, stop_id,
    ];
    booking_rules: BookingRuleRow = "booking_rules.txt" required [booking_rule_id, booking_type] [
        booking_rule_id, booking_type, prior_notice_duration_min, prior_notice_duration_max,
        prior_notice_last_day, prior_notice_last_time, prior_notice_start_day,
        prior_notice_start_time, prior_notice_service_id, message, pickup_message,
        drop_off_message, phone_number, info_url, booking_url,
    ];
    translations: TranslationRow = "translations.txt" required [table_name, field_name, language, translation] [
        table_name, field_name, language, translation, record_id, record_sub_id, field_value,
    ];
    feed_info: FeedInfoRow = "feed_info.txt" required [feed_publisher_name, feed_publisher_url, feed_lang] [
        feed_publisher_name, feed_publisher_url, feed_lang, default_lang, feed_start_date,
        feed_end_date, feed_version, feed_contact_email, feed_contact_url,
    ];
    attributions: AttributionRow = "attributions.txt" required [organization_name] [
        attribution_id, agency_id, route_id, trip_id, organization_name, is_producer, is_operator,
        is_authority, attribution_url, attribution_email, attribution_phone,
    ];
}

fn has_member(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> bool {
    archive.by_name(name).is_ok()
}

/// One member as UTF-8 text, BOM stripped; `None` when absent.
fn read_member(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &'static str) -> Result<Option<String>, FeedError> {
    let mut file = match archive.by_name(name) {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(FeedError::Zip(e.to_string())),
    };
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|e| FeedError::Zip(e.to_string()))?;
    let text = String::from_utf8(bytes).map_err(|e| FeedError::Csv { file: name, message: e.to_string() })?;
    Ok(Some(match text.strip_prefix('\u{feff}') {
        Some(rest) => rest.to_string(),
        None => text,
    }))
}

fn read_table<T: Table>(archive: &mut zip::ZipArchive<Cursor<&[u8]>>) -> Result<Vec<T>, FeedError> {
    let Some(text) = read_member(archive, T::FILE)? else { return Ok(Vec::new()) };
    // Ragged rows are common in real feeds (trailing separators,
    // short optional tails): missing cells read as empty.
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::Headers)
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for row in reader.deserialize() {
        rows.push(row.map_err(|e| FeedError::Csv { file: T::FILE, message: e.to_string() })?);
    }
    Ok(rows)
}

/// A file's CSV text: the header holds every required column and
/// every column some row fills, in spec order; `\r\n` line endings
/// and RFC 4180 quoting, as the spec asks.
fn render<T: Table>(rows: &[T]) -> Vec<u8> {
    let cells: Vec<Vec<Option<&str>>> = rows.iter().map(Table::cells).collect();
    let used: Vec<usize> = (0..T::COLUMNS.len())
        .filter(|&i| T::REQUIRED.contains(&T::COLUMNS[i]) || cells.iter().any(|row| row[i].is_some()))
        .collect();
    let mut text = String::new();
    line(&mut text, used.iter().map(|&i| T::COLUMNS[i]));
    for row in &cells {
        line(&mut text, used.iter().map(|&i| row[i].unwrap_or("")));
    }
    text.into_bytes()
}

fn line<'a>(out: &mut String, cells: impl Iterator<Item = &'a str>) {
    for (i, cell) in cells.enumerate() {
        if i > 0 {
            out.push(',');
        }
        if cell.contains([',', '"', '\n', '\r']) {
            out.push('"');
            out.push_str(&cell.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(cell);
        }
    }
    out.push_str("\r\n");
}

/// A zip whose bytes carry no wall clock: every entry gets the DOS
/// epoch, so the output is a pure function of the content.
struct ZipWriter {
    writer: zip::ZipWriter<Cursor<Vec<u8>>>,
}

impl ZipWriter {
    fn new() -> ZipWriter {
        ZipWriter { writer: zip::ZipWriter::new(Cursor::new(Vec::new())) }
    }

    fn add(&mut self, name: &str, contents: &[u8]) {
        let options =
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        self.writer.start_file(name, options).expect("zip entry header serializes");
        self.writer.write_all(contents).expect("zip entry body serializes");
    }

    fn finish(self) -> Vec<u8> {
        self.writer.finish().expect("zip central directory serializes").into_inner()
    }
}

/// A feed's counts at a glance.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct FeedSummary {
    pub agencies: usize,
    pub routes: usize,
    pub stops: usize,
    pub trips: usize,
    pub stop_times: usize,
}

/// `feed_info.txt`: which dataset this is, and who publishes it.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct FeedInfo {
    pub feed_publisher_name: Option<String>,
    pub feed_publisher_url: Option<String>,
    pub feed_lang: Option<String>,
    pub default_lang: Option<String>,
    pub feed_start_date: Option<String>,
    pub feed_end_date: Option<String>,
    pub feed_version: Option<String>,
    pub feed_contact_email: Option<String>,
    pub feed_contact_url: Option<String>,
}

impl Feed {
    pub fn summary(&self) -> FeedSummary {
        FeedSummary {
            agencies: self.agency.len(),
            routes: self.routes.len(),
            stops: self.stops.len(),
            trips: self.trips.len(),
            stop_times: self.stop_times.len(),
        }
    }

    /// The feed's `feed_info.txt` row, when it has one.
    pub fn info(&self) -> Option<FeedInfo> {
        self.feed_info.first().map(|r| FeedInfo {
            feed_publisher_name: r.feed_publisher_name.clone(),
            feed_publisher_url: r.feed_publisher_url.clone(),
            feed_lang: r.feed_lang.clone(),
            default_lang: r.default_lang.clone(),
            feed_start_date: r.feed_start_date.clone(),
            feed_end_date: r.feed_end_date.clone(),
            feed_version: r.feed_version.clone(),
            feed_contact_email: r.feed_contact_email.clone(),
            feed_contact_url: r.feed_contact_url.clone(),
        })
    }
}

/// Standard base64 (with padding) — how zip bytes cross the JSON seam.
pub fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/e2e/fixtures/sample-gtfs.zip"))
            .expect("fixture exists")
    }

    #[test]
    fn reads_and_rewrites_the_fixture_unchanged() {
        let feed = Feed::from_zip_bytes(&fixture()).expect("fixture reads");
        assert!(!feed.trips.is_empty() && !feed.stop_times.is_empty());
        let again = Feed::from_zip_bytes(&feed.to_zip_bytes()).expect("written zip reads");
        assert_eq!(again, feed);
    }

    #[test]
    fn writes_deterministic_bytes() {
        let feed = Feed::from_zip_bytes(&fixture()).expect("fixture reads");
        assert_eq!(feed.to_zip_bytes(), feed.to_zip_bytes());
    }

    #[test]
    fn a_required_but_empty_column_is_still_written() {
        let mut feed = Feed::default();
        feed.fare_attributes.push(FareAttributeRow {
            fare_id: Some("f".into()),
            price: Some("1".into()),
            currency_type: Some("USD".into()),
            payment_method: Some("0".into()),
            ..Default::default()
        });
        let text = String::from_utf8(render(&feed.fare_attributes)).unwrap();
        assert!(text.starts_with("fare_id,price,currency_type,payment_method,transfers\r\n"), "{text}");
    }
}
