// Real-data soak: the Clark County Trapeze roads export. The file is
// ~100 MB and lives outside the repo, so the test is gated on
// COUNTY_GEOJSON pointing at the downloaded geojson — it skips
// silently otherwise, keeping `just check` hermetic.
//
// What it proves that the synthetic fixtures cannot: the artifact
// parses a production-sized network, and one-way travel is respected
// on real digitized geometry (NW 10TH AVE is `>` northbound; the
// southbound trip must loop through the grid, never the segment).

#[test]
fn county_export_parses_and_respects_one_ways() {
    let Ok(path) = std::env::var("COUNTY_GEOJSON") else {
        eprintln!("skipping: COUNTY_GEOJSON not set");
        return;
    };
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let t0 = std::time::Instant::now();
    let net = transit_streets::StreetNetwork::parse(&bytes).expect("parse");
    let elapsed = t0.elapsed();
    println!(
        "{} segments / {} nodes in {elapsed:?}",
        net.segment_count(),
        net.node_count()
    );
    assert!(net.segment_count() > 500_000, "production-sized, not a stub");

    // NW 10TH AVE, OneWay ">": digitized
    // (-122.681412333, 45.530829395) -> (-122.681435787, 45.531537199)
    // — legal travel is northbound only.
    let south = net.snap(45.53085, -122.68141).expect("snap south");
    let north = net.snap(45.53150, -122.68144).expect("snap north");
    assert_eq!(south.seg, north.seg, "both on NW 10TH AVE");
    assert!(north.t > south.t, "north is ahead in digitized order");
    assert_eq!(
        net.route(south, north).map(|r| r.len()),
        Some(1),
        "with the grain: the same-segment shortcut"
    );
    let back = net.route(north, south).expect("the grid offers a legal loop");
    println!("against the grain: {} points", back.len());
    assert!(back.len() > 1, "no direct backward shortcut on a one-way");
    for p in &back {
        let on_line = (p.lon - -122.68142).abs() < 5e-6;
        let mid_segment =
            (p.lat - south.lat).abs() > 1e-6 && (p.lat - north.lat).abs() > 1e-6;
        assert!(
            !(on_line && mid_segment),
            "backward interior point on the one-way: {p:?}"
        );
    }
}
