//! Loading a GTFS static feed from an in-memory zip.
//!
//! The entry point is [`from_zip_bytes`]. Everything here is deliberately
//! byte-oriented: the browser (or a native app) owns networking and hands
//! us the raw zip archive, so this code has no I/O dependencies and
//! compiles anywhere — including wasm32.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use serde::de::DeserializeOwned;

use crate::calendar::ServiceCalendar;
use crate::error::{GtfsError, Result};
use crate::model::{Agency, Calendar, CalendarDate, FeedInfo, Route, ShapePoint, Stop, StopTime, Transfer, Trip};
use crate::time::parse_time;
use crate::Gtfs;

/// A raw `stop_times.txt` row before time conversion.
#[derive(Debug, serde::Deserialize)]
struct RawStopTime {
    trip_id: String,
    stop_id: String,
    stop_sequence: u32,
    #[serde(default)]
    arrival_time: Option<String>,
    #[serde(default)]
    departure_time: Option<String>,
    #[serde(default)]
    stop_headsign: Option<String>,
    #[serde(default)]
    timepoint: Option<String>,
    #[serde(default)]
    shape_dist_traveled: Option<String>,
}

/// Read one member of the archive as UTF-8 text.
/// `Ok(None)` means the file is not in the feed.
fn read_member(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Option<String>> {
    match archive.by_name(name) {
        Ok(mut file) => {
            let mut text = String::new();
            file.read_to_string(&mut text)?;
            Ok(Some(text))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Like [`read_member`], but the file is required.
fn require(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &'static str) -> Result<String> {
    read_member(archive, name)?.ok_or(GtfsError::MissingFile(name))
}

/// Deserialize a whole CSV member into `Vec<T>`.
///
/// `flexible(true)` allows rows with fewer/more columns than the header —
/// real feeds do this constantly (trailing separators, ragged optional
/// columns), and serde ignores unknown columns on the right.
fn parse_csv<T: DeserializeOwned>(file: &'static str, text: &str) -> Result<Vec<T>> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(text.as_bytes());

    let mut rows = Vec::new();
    for record in reader.deserialize() {
        match record {
            Ok(row) => rows.push(row),
            Err(source) => return Err(GtfsError::Csv { file, source }),
        }
    }
    Ok(rows)
}

/// Insert into an id-keyed map, refusing duplicate ids.
fn insert_unique<V>(
    map: &mut HashMap<String, V>,
    value: V,
    get_id: impl Fn(&V) -> &str,
    file: &'static str,
) -> Result<()> {
    let id = get_id(&value).to_string();
    if map.contains_key(&id) {
        return Err(GtfsError::Invalid { file, message: format!("duplicate id {id:?}") });
    }
    map.insert(id, value);
    Ok(())
}

fn parse_opt_time(field: &'static str, raw: &Option<String>) -> Result<Option<u32>> {
    match raw.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(text) => parse_time(text).map(Some).ok_or_else(|| GtfsError::Invalid {
            file: "stop_times.txt",
            message: format!("bad {field} value {text:?}"),
        }),
    }
}

/// Parse a complete GTFS static feed from the bytes of a `.zip` archive.
pub fn from_zip_bytes(bytes: &[u8]) -> Result<Gtfs> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;

    let agencies: Vec<Agency> = parse_csv("agency.txt", &require(&mut archive, "agency.txt")?)?;
    let routes: Vec<Route> = parse_csv("routes.txt", &require(&mut archive, "routes.txt")?)?;
    let stops: Vec<Stop> = parse_csv("stops.txt", &require(&mut archive, "stops.txt")?)?;
    let trips: Vec<Trip> = parse_csv("trips.txt", &require(&mut archive, "trips.txt")?)?;
    let raw_stop_times: Vec<RawStopTime> =
        parse_csv("stop_times.txt", &require(&mut archive, "stop_times.txt")?)?;

    // calendar.txt and calendar_dates.txt are individually optional, but
    // the spec requires at least one of the two.
    let calendars: Vec<Calendar> = match read_member(&mut archive, "calendar.txt")? {
        Some(text) => parse_csv("calendar.txt", &text)?,
        None => Vec::new(),
    };    let calendar_dates: Vec<CalendarDate> =
        match read_member(&mut archive, "calendar_dates.txt")? {
            Some(text) => parse_csv("calendar_dates.txt", &text)?,
            None => Vec::new(),
        };
    if calendars.is_empty() && calendar_dates.is_empty() {
        return Err(GtfsError::Invalid {
            file: "calendar.txt",
            message: "feed has neither calendar.txt nor calendar_dates.txt".into(),
        });
    }

    // feed_info.txt is optional metadata; feeds are inconsistent about
    // which columns appear, so every field parses as Option.
    let feed_info: Option<FeedInfo> = match read_member(&mut archive, "feed_info.txt")? {
        Some(text) => {
            let rows: Vec<FeedInfo> = parse_csv("feed_info.txt", &text)?;
            rows.into_iter().next()
        }
        None => None,
    };

    // shapes.txt is optional; group its points by shape_id and order them
    // by sequence so they read as a polyline.
    let shape_rows: Vec<ShapePoint> = match read_member(&mut archive, "shapes.txt")? {
        Some(text) => parse_csv("shapes.txt", &text)?,
        None => Vec::new(),
    };
    let mut shapes: HashMap<String, Vec<ShapePoint>> = HashMap::with_capacity(shape_rows.len());
    for point in shape_rows {
        shapes.entry(point.shape_id.clone()).or_default().push(point);
    }
    for points in shapes.values_mut() {
        points.sort_by_key(|p| p.shape_pt_sequence);
    }

    // transfers.txt is optional: declared transfers between stops.
    // Kept faithful (pairs and types); the authored network derives
    // its transfer-point flags from these.
    let transfers: Vec<Transfer> = match read_member(&mut archive, "transfers.txt")? {
        Some(text) => parse_csv("transfers.txt", &text)?,
        None => Vec::new(),
    };

    // Convert raw stop times: string times -> seconds since service-day
    // midnight.
    let mut stop_times = Vec::with_capacity(raw_stop_times.len());
    for raw in raw_stop_times {
        // timepoint: absent or empty means exact (the spec's
        // default); 0 marks an interpolated stop.
        let timepoint = match raw.timepoint.as_deref() {
            None | Some("") | Some("1") => true,
            Some("0") => false,
            other => {
                return Err(GtfsError::Invalid {
                    file: "stop_times.txt",
                    message: format!("timepoint must be 0 or 1, got {other:?}"),
                })
            }
        };
        let shape_dist_traveled = raw
            .shape_dist_traveled
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.parse::<f64>().map_err(|_| GtfsError::Invalid {
                    file: "stop_times.txt",
                    message: format!("shape_dist_traveled is not a number: {s:?}"),
                })
            })
            .transpose()?;
        stop_times.push(StopTime {
            arrival_time: parse_opt_time("arrival_time", &raw.arrival_time)?,
            departure_time: parse_opt_time("departure_time", &raw.departure_time)?,
            trip_id: raw.trip_id,
            stop_id: raw.stop_id,
            stop_sequence: raw.stop_sequence,
            stop_headsign: raw.stop_headsign,
            timepoint,
            shape_dist_traveled,
        });
    }

    // Id-keyed maps.
    let mut routes_map = HashMap::with_capacity(routes.len());
    for r in routes {
        insert_unique(&mut routes_map, r, |r| &r.route_id, "routes.txt")?;
    }
    let mut stops_map = HashMap::with_capacity(stops.len());
    for s in stops {
        insert_unique(&mut stops_map, s, |s| &s.stop_id, "stops.txt")?;
    }
    let mut trips_map = HashMap::with_capacity(trips.len());
    for t in trips {
        insert_unique(&mut trips_map, t, |t| &t.trip_id, "trips.txt")?;
    }

    // Secondary index: for each stop, the positions in `stop_times` that
    // touch it. This is what makes departure lookups fast without
    // scanning the whole table.
    let mut stop_times_by_stop: HashMap<String, Vec<u32>> = HashMap::new();
    for (i, st) in stop_times.iter().enumerate() {
        stop_times_by_stop
            .entry(st.stop_id.clone())
            .or_default()
            .push(i as u32);
    }

    // Secondary index: each trip's stop times, in stop_sequence order —
    // the vehicle's path from first to last stop.
    let mut stop_times_by_trip: HashMap<String, Vec<u32>> = HashMap::new();
    for (i, st) in stop_times.iter().enumerate() {
        stop_times_by_trip
            .entry(st.trip_id.clone())
            .or_default()
            .push(i as u32);
    }
    for indices in stop_times_by_trip.values_mut() {
        indices.sort_by_key(|&i| stop_times[i as usize].stop_sequence);
    }

    // Secondary index: block_id → member trip ids (blocks = the chains of
    // trips one vehicle runs per day).
    let mut blocks: HashMap<String, Vec<String>> = HashMap::new();
    for trip in trips_map.values() {
        if let Some(block_id) = &trip.block_id {
            blocks.entry(block_id.clone()).or_default().push(trip.trip_id.clone());
        }
    }

    Ok(Gtfs {
        agencies,
        routes: routes_map,
        stops: stops_map,
        trips: trips_map,
        stop_times,
        services: ServiceCalendar::new(calendars, calendar_dates),
        stop_times_by_stop,
        stop_times_by_trip,
        blocks,
        shapes,
        transfers,
        feed_info,
    })
}
