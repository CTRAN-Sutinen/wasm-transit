//! Shared fixture-building helpers for transit-router tests.
use std::io::Write;

use chrono::NaiveDate;

#[derive(Debug, Clone)]
pub struct TripSpec {
    pub stops: Vec<u32>,
    pub start: u32,
    pub step: u32,
}

/// A fixture feed through the sanctioned path: the codec's import
/// (feed → network document), then the model's own materialize for
/// the date — the same composition the capability performs, never
/// the GTFS substrate in router code.
pub fn model_day(
    zip: &[u8],
    date: NaiveDate,
) -> (transit_network::Network, Vec<transit_network::MaterializedTrip>) {
    let feed = transit_codec::Feed::from_zip_bytes(zip).expect("parse");
    let outcome = transit_codec::import(&feed);
    let day = transit_network::materialize(&outcome.network, date).expect("materialize");
    (outcome.network, day)
}

/// Serialize a generated spec into a GTFS zip.
#[allow(dead_code)]
pub fn build_zip(n_stops: usize, trips: &[TripSpec]) -> Vec<u8> {
    let timed: Vec<(Vec<u32>, Vec<u32>)> = trips
        .iter()
        .map(|t| (t.stops.clone(), (0..t.stops.len() as u32).map(|seq| t.start + seq * t.step).collect()))
        .collect();
    build_zip_timed(n_stops, 0.0008, &timed)
}

/// [`build_zip`] with the stops `spacing_deg` of latitude apart
/// (0.0008° ≈ 89 m: every near pair walkable; 0.003° ≈ 333 m: no
/// footpaths at all).
#[allow(dead_code)]
pub fn build_zip_spaced(n_stops: usize, spacing_deg: f64, trips: &[TripSpec]) -> Vec<u8> {
    let timed: Vec<(Vec<u32>, Vec<u32>)> = trips
        .iter()
        .map(|t| (t.stops.clone(), (0..t.stops.len() as u32).map(|seq| t.start + seq * t.step).collect()))
        .collect();
    build_zip_timed(n_stops, spacing_deg, &timed)
}

/// Serialize trips with explicit per-visit times (arrival =
/// departure) into a GTFS zip: trip `i` is `t{i}`, stop `n` is
/// `s{n}`, one route, one every-day service.
#[allow(dead_code)]
pub fn build_zip_timed(n_stops: usize, spacing_deg: f64, trips: &[(Vec<u32>, Vec<u32>)]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let add = |w: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, body: String| {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    };

    add(&mut w, "agency.txt", "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n".into());
    add(&mut w, "routes.txt", "route_id,agency_id,route_short_name,route_type\n1,1,R,3\n".into());

    let mut stops = String::from("stop_id,stop_name,stop_lat,stop_lon\n");
    for i in 0..n_stops {
        let lat = 45.0 + (i as f64) * spacing_deg;
        stops.push_str(&format!("s{i},Stop {i},{lat:.6},-122.500000\n"));
    }
    add(&mut w, "stops.txt", stops);

    add(&mut w, "calendar.txt",
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nALL,1,1,1,1,1,1,1,20260101,20261231\n".into());

    let mut trips_txt = String::from("route_id,service_id,trip_id\n");
    let mut stop_times = String::from("trip_id,arrival_time,departure_time,stop_id,stop_sequence\n");
    for (i, (stops, times)) in trips.iter().enumerate() {
        trips_txt.push_str(&format!("1,ALL,t{i}\n"));
        for (seq, (&s, &secs)) in stops.iter().zip(times).enumerate() {
            let hh = secs / 3600;
            let mm = (secs % 3600) / 60;
            let ss = secs % 60;
            stop_times.push_str(&format!(
                "t{i},{hh:02}:{mm:02}:{ss:02},{hh:02}:{mm:02}:{ss:02},s{s},{}\n",
                seq + 1
            ));
        }
    }
    add(&mut w, "trips.txt", trips_txt);
    add(&mut w, "stop_times.txt", stop_times);

    w.finish().unwrap().into_inner()
}

/// Unreachable in [`oracle_arrival`].
#[allow(dead_code)]
pub const INF: u32 = u32::MAX;

/// The deliberately dumb earliest-arrival oracle: Bellman-Ford-style
/// relaxation over every (trip, board, alight) triple and every
/// footpath of the built network, to a fixpoint, with no leg bound.
/// It shares no code with the round-based search.
#[allow(dead_code)]
pub fn oracle_arrival(net: &transit_router::RouterNetwork, from: usize, to: usize, depart: u32) -> u32 {
    let n = net.stops.len();
    let mut arr = vec![INF; n];
    arr[from] = depart;
    loop {
        let mut changed = false;
        for pattern in &net.patterns {
            for trip in &pattern.trips {
                for i in 0..pattern.stop_positions.len() {
                    let si = pattern.stop_positions[i] as usize;
                    if arr[si] == INF || arr[si] > trip.departures[i] {
                        continue;
                    }
                    for j in i + 1..pattern.stop_positions.len() {
                        let sj = pattern.stop_positions[j] as usize;
                        if trip.arrivals[j] < arr[sj] {
                            arr[sj] = trip.arrivals[j];
                            changed = true;
                        }
                    }
                }
            }
        }
        for (s, node) in net.stops.iter().enumerate() {
            if arr[s] == INF {
                continue;
            }
            for edge in &node.footpaths {
                let candidate = arr[s] + edge.walk_secs;
                let t = edge.to as usize;
                if candidate < arr[t] {
                    arr[t] = candidate;
                    changed = true;
                }
            }
        }
        if !changed {
            return arr[to];
        }
    }
}

/// An itinerary makes chronological sense from `depart`: legs connect
/// stop to stop, nothing boards before the previous leg ended, every
/// transit leg alights at or after it boards, and the last leg ends
/// at the promised arrival at the destination.
#[allow(dead_code)]
pub fn assert_feasible(it: &transit_router::Itinerary, depart: u32) {
    use transit_router::Leg;
    assert_eq!(it.depart_secs, depart, "the itinerary leaves when it says");
    let mut t = depart;
    let mut place: &str = &it.from_stop_id;
    for leg in &it.legs {
        match leg {
            Leg::Walk { from_stop_id, to_stop_id, walk_secs, .. } => {
                assert_eq!(place, from_stop_id, "walk starts where the last leg ended");
                t += walk_secs;
                place = to_stop_id;
            }
            Leg::Transit { board_stop_id, board_secs, alight_stop_id, alight_secs, .. } => {
                assert_eq!(place, board_stop_id, "transit boards where the last leg ended");
                assert!(*board_secs >= t, "boards at {board_secs} before the previous leg ended at {t}");
                assert!(*alight_secs >= *board_secs, "alights before boarding");
                t = *alight_secs;
                place = alight_stop_id;
            }
        }
    }
    assert_eq!(place, it.to_stop_id, "the itinerary ends at the destination");
    assert_eq!(t, it.arrival_secs, "the last leg ends at the promised arrival");
}
