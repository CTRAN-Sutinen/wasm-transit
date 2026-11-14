// Route 99 (C-TRAN 2045 plan concept): Washougal – Camas – Lacamas Lake
// – East Fourth Plain. The data every step of the test reads.

/// The stops, Washougal first: id, name, lat, lon, metres and seconds
/// from Washougal TC.
export const STOPS = [
  ["9906", "Washougal Transit Center", 45.58, -122.348, 0, 0],
  ["9905", "NE 4th Ave & Everett St (Camas)", 45.5867, -122.404, 4400, 540],
  ["9904", "NW Lake Rd & Lacamas Lake", 45.608, -122.423, 7200, 900],
  ["9903", "NE Ingle Rd & NE Goodwin Rd", 45.648, -122.43, 11700, 1500],
  ["9902", "Fourth Plain & NE 192nd Ave", 45.6727, -122.469, 15800, 1980],
  ["9901", "East Fourth Plain Transit Center", 45.6727, -122.506, 18700, 2400],
];
export const LENGTH_M = 18700;
export const RUN_SECS = 2400;

export const ROUTE = { id: "099", long_name: "Washougal/Camas/East Fourth Plain" };

/// Service key, first and last start hour, minutes between trips. Trips
/// to Washougal start 15 minutes after the hour.
export const SERVICE = [
  ["1-WKDY", 6, 20, 30],
  ["2-SAT", 7, 19, 60],
  ["3-SUN", 7, 19, 60],
];

/// The run's dates, from today (the host's date: run on a machine in
/// the agency's zone). The stop's life is backdated so it is in service
/// today and the pick can take effect today.
function iso(d) {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}
function daysAgo(n) {
  const d = new Date();
  d.setDate(d.getDate() - n);
  return iso(d);
}
export const DATES = {
  planned: daysAgo(73),
  permitted: daysAgo(43),
  built: daysAgo(11),
  in_service: daysAgo(0),
  today: daysAgo(0),
};
