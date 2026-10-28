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
pub fn build_zip(n_stops: usize, trips: &[TripSpec]) -> Vec<u8> {
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
        let lat = 45.0 + (i as f64) * 0.0008;
        stops.push_str(&format!("s{i},Stop {i},{lat:.6},-122.500000\n"));
    }
    add(&mut w, "stops.txt", stops);

    add(&mut w, "calendar.txt",
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nALL,1,1,1,1,1,1,1,20260101,20261231\n".into());

    let mut trips_txt = String::from("route_id,service_id,trip_id\n");
    let mut stop_times = String::from("trip_id,arrival_time,departure_time,stop_id,stop_sequence\n");
    for (i, t) in trips.iter().enumerate() {
        trips_txt.push_str(&format!("1,ALL,t{i}\n"));
        for (seq, &s) in t.stops.iter().enumerate() {
            let secs = t.start + seq as u32 * t.step;
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
