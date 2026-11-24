# transit-disruptions

What is disrupted on a day, and whether an alert explains it (D-213, D-218). A disruption comes from the edit document (a submitted or accepted diversion or trip cancel in force), the live document (a trip instance the feed cancels), or the stops document (a suspended stop). Each is listed as its document gives it, with its origin. A trip that both the edit document and the feed cancel is listed twice, and no rule picks one. cap-disruptions is the seam.

- `disruptions` gives the list. Each carries the names a rider uses (the route; a cancelled trip's start, first stop, and headsign; the stops it takes away, each with where it is), the next trip from the same first stop, the alerts that explain it, and its latest note. A closed stop is where the stops document puts it on the day; every other stop is where the network puts it.
- `gaps` gives what no alert explains, with what an alert about each starts from, and the riders it informs: a cancelled trip's instance on its route; a detour's route, and its route at each stop it skips; a closed stop.
- `start_alert` is what a new alert starts from (D-228): for a gap, its effect, the riders it informs, and what it explains, with the stop the rider preview opens at. With no gap, a blank alert, told from `now`.
- `today` gives the day by route: each route with a disruption or a late trip, its late trips from the live document's statuses, and how many routes have neither. A suspended stop is listed on every route that serves it. A filter answers one route or one stop. A station is one place with its bays.

## Why

Each entry covers one choice that other code depends on. The heading
names the code it governs. **Holds** is what must stay true. Change it
only with the user, after you read **Because**.

### What is disrupted is one question over every source (transit-disruptions, cap-disruptions)

- **Holds:** the edit document, the live feed, and the stops document each give their own disruptions, and each is listed as given, with its origin. A trip that two sources cancel is listed twice. No rule picks a winner.
- **Holds:** the gap list and the day by route are one capability's facets. The alerts capability answers what the agency tells riders, and no longer what is unexplained.
- **Holds:** each disruption carries the names a rider uses, so a page shows "2:10 PM from Fisher's Landing TC" without a join.
- **Because:** the live feed is where C-TRAN's cancels come from (D-213), and customer service answers a rider's question about a route or a stop from one list. An op on cap-alerts that read the live document and the notes would be the second document of the split rule (`crates/AGENTS.md`).
- **Rejected:** `today` on cap-alerts (D-215); a page that joins the five documents.
- **Decided:** D-213, D-218.
