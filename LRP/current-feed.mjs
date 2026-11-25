// The current C-TRAN feed as the plan's Appendix A table (scenarios.csv),
// one row per route and day type, so a scenario diff is a join on
// (route, day).
//
//   node LRP/current-feed.mjs [feed.zip] [weekday sat sun] > LRP/current-feed.csv
//
// Defaults are testdata/ctran-gtfs.zip and an ordinary week of the
// 2026-09-13 to 2027-01-09 feed: Wed 2026-11-18, Sat 2026-11-21,
// Sun 2026-11-22. The columns are measured from the feed, not taken from
// the plan's method:
//   first_trip / last_trip  the first and last trip start (either direction)
//   peak_headway_min        60 / the most trips one direction starts in
//                           any clock hour (rounded)
//   peak_vehicles           the most blocks on the route at once. A
//                           block's trips on the route form one interval
//                           while the gap between them is under 60 min.
//   trips                   every trip that day, both directions
import { execFileSync } from "node:child_process";

const [zip = "testdata/ctran-gtfs.zip", ...dates] = process.argv.slice(2);
const DAYS = [["weekday", dates[0] ?? "2026-11-18"], ["saturday", dates[1] ?? "2026-11-21"], ["sunday", dates[2] ?? "2026-11-22"]];

function table(name) {
  const text = execFileSync("unzip", ["-p", zip, name], { maxBuffer: 1 << 30 }).toString().replace(/^﻿/, "");
  const lines = text.split(/\r?\n/).filter(Boolean);
  const head = lines[0].split(",");
  return lines.slice(1).map((l) => {
    const cells = l.split(",").map((c) => c.replace(/^"|"$/g, ""));
    return Object.fromEntries(head.map((h, i) => [h, cells[i]]));
  });
}
const secs = (t) => { const [h, m, s] = t.split(":").map(Number); return h * 3600 + m * 60 + s; };
const clock = (s) => {
  const h = Math.floor(s / 3600) % 24, m = Math.floor((s % 3600) / 60);
  return `${h % 12 || 12}:${String(m).padStart(2, "0")} ${h < 12 ? "AM" : "PM"}`;
};

const routes = new Map(table("routes.txt").map((r) => [r.route_id, r]));
const trips = table("trips.txt");
const span = new Map();
for (const st of table("stop_times.txt")) {
  const t = st.departure_time || st.arrival_time;
  if (!t) continue;
  const s = secs(t), cur = span.get(st.trip_id);
  if (!cur) span.set(st.trip_id, [s, s]);
  else { if (s < cur[0]) cur[0] = s; if (s > cur[1]) cur[1] = s; }
}
// A service runs on the weekdays of calendar.txt (all zero in this feed),
// changed by the exceptions in calendar_dates.txt.
const cal = table("calendar.txt"), cd = table("calendar_dates.txt");
const WD = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"];
function services(date) {
  const ymd = date.replaceAll("-", ""), wd = WD[new Date(date + "T12:00:00Z").getUTCDay()];
  const on = new Set(cal.filter((c) => c[wd] === "1" && c.start_date <= ymd && ymd <= c.end_date).map((c) => c.service_id));
  for (const e of cd.filter((e) => e.date === ymd)) e.exception_type === "1" ? on.add(e.service_id) : on.delete(e.service_id);
  return on;
}

const out = [["route", "short_name", "name", "network", "day", "date", "first_trip", "last_trip", "peak_headway_min", "peak_vehicles", "trips"]];
for (const [day, date] of DAYS) {
  const on = services(date);
  const byRoute = new Map();
  for (const t of trips) if (on.has(t.service_id) && span.has(t.trip_id)) {
    if (!byRoute.has(t.route_id)) byRoute.set(t.route_id, []);
    byRoute.get(t.route_id).push(t);
  }
  for (const [rid, rt] of [...byRoute].sort()) {
    const starts = rt.map((t) => span.get(t.trip_id)[0]);
    const perHour = new Map();
    for (const t of rt) {
      const k = `${t.direction_id}:${Math.floor(span.get(t.trip_id)[0] / 3600)}`;
      perHour.set(k, (perHour.get(k) ?? 0) + 1);
    }
    const most = Math.max(...perHour.values());
    const blocks = new Map();
    for (const t of rt) {
      const b = t.block_id || t.trip_id;
      if (!blocks.has(b)) blocks.set(b, []);
      blocks.get(b).push(span.get(t.trip_id));
    }
    const events = [];
    for (const iv of blocks.values()) {
      iv.sort((a, b) => a[0] - b[0]);
      let [s, e] = iv[0];
      for (const [s2, e2] of iv.slice(1)) {
        if (s2 - e < 3600) e = Math.max(e, e2);
        else { events.push([s, 1], [e, -1]); [s, e] = [s2, e2]; }
      }
      events.push([s, 1], [e, -1]);
    }
    events.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
    let now = 0, peak = 0;
    for (const [, d] of events) { now += d; peak = Math.max(peak, now); }
    const r = routes.get(rid) ?? {};
    out.push([rid, r.route_short_name, r.route_long_name, r.network_id, day, date,
      clock(Math.min(...starts)), clock(Math.max(...starts)), Math.round(60 / most), peak, rt.length]);
  }
}
const q = (c) => (/[",]/.test(String(c)) ? `"${String(c).replaceAll('"', '""')}"` : c);
process.stdout.write(out.map((r) => r.map(q).join(",")).join("\n") + "\n");
