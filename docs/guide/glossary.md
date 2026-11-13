# Glossary

The terms used in this guide and in the rest of the documentation,
in alphabetical order. Where an agency usually has its own word for
something, the entry gives it.

Each entry links to where the term is explained in full, when that
helps.

---

**Accepted, draft, submitted, retired.** The stages of a detour or
trip cancellation entered by staff. A draft is being written. A
submitted record is waiting for review. An accepted record is in
effect. A retired record no longer applies, but stays in the history.

**Alignment.** The line a route follows on the ground. Agencies may
call it the route path or the shape. In GTFS it is a `shapes.txt`
shape. Several patterns can share one alignment.

**Assembly.** The single module an app loads, made by joining the
capabilities that app uses. Joining them means each document is
loaded once per page. See [architecture/bus.md](../architecture/bus.md).

**Block.** The sequence of trips one vehicle runs in a day, as in
most scheduling systems. GTFS calls it `block_id`.

**Blob.** A document stored by its content hash, at
`blobs/<hash>`. Both the server and a static deployment serve
documents this way.

**Board.** A list of departures at a stop. The board capability
answers "when does service leave this stop?"

**Boundary.** The parts of a GTFS feed that the network model does
not use but must keep. Keeping them lets the feed be exported
unchanged, and lets the live feeds be matched to it. An example is
the stop numbering within each trip that GTFS-realtime refers to.

**Build.** One version of the app's code: its pages, scripts, and
modules. A new build replaces the app's code. A new schedule does not
need a new build.

**Capability.** A module that answers one kind of question, such as
board, router, or directory. `node tools/caps-run.mjs list` lists
them all. Each has its own list of requests, called **ops**. See
[architecture/crates.md](../architecture/crates.md).

**caps-run.** A command-line tool that runs the same modules the apps
run. Developers and AI agents use it to ask questions directly and to
test the software on real data. See
[architecture/registry.md](../architecture/registry.md).

**Content hash.** A long code calculated from a document's exact
bytes, like a fingerprint. The same contents always give the same
hash. A one-character change gives a different hash. The software
uses the hash as the document's name. It is written
`sha256:` followed by 64 characters.

**Core.** The part of every module that loads, checks, holds, and
drops documents. The code and the technical documents call it the
**kernel**.

**Decision.** A question the project's owner has been asked, recorded
in [DECISIONS.md](../DECISIONS.md) with a number such as `D-77`. The
record is kept for good, with the answer and its date.

**Deployment.** One running copy of an app (or of the server) for
one agency, at one web address.

**Detour.** A change to the stops a trip serves on given days. The
technical documents call it a **diversion**.

**Direction.** One of the two ways a route runs, for example
"to Downtown" and "to the Airport". GTFS calls it `direction_id`.

**Document.** One piece of data in a file, named by its content hash:
the network, a live-feed snapshot, the edit document, a rider's
saved stops. Documents are plain JSON or an open standard format.

**Dogfood.** Using the software on real data, the way its users
would, to find problems that test data hides. Here it usually means
running C-TRAN's feed through caps-run.

**Draft (schedule).** A pick a planner is still changing, kept on
their own computer. It runs nowhere until it is published; once
published it never changes in place.

**Edit document.** The shared list of detours and trip cancellations
that staff have entered. Each entry is a **record**. Only a
deployment with a server has one.

**Engine.** The one part of the browser code that sends requests to
modules. Every other part of an app asks the engine. See
[architecture/web.md](../architecture/web.md).

**Export.** A rider app written out as a folder of plain files, ready
for a static web host, with no server needed. See
[architecture/static-deployment.md](../architecture/static-deployment.md).

**Finding.** A problem found on real data, recorded in
[QA-FINDINGS.md](../QA-FINDINGS.md) with a number such as `QAF-63`.
It is removed when it is fixed.

**Fork.** A copy of the network that a planner changes to make a
future schedule. Publishing a fork makes it a **pick**.

**Geocoder.** An outside service that turns an address into a place
on the map. The server can pass address searches to one. The
software shows its answers as they come and does not store them.

**GTFS.** The General Transit Feed Specification: the standard file
format agencies use to publish their schedules. wasm-transit reads it
and writes it, but does not use it internally.

**GTFS-realtime (GTFS-rt).** The standard format for live data: trip
delays, vehicle positions, and service alerts. wasm-transit reads it.

**Host.** A program that runs modules: the browser, the server, or
caps-run. A host fetches files, reads the clock, saves data, and draws
the screen. It never calculates an answer itself.

**Kernel.** See **Core**.

**Latitude and longitude.** The two numbers that say where a point is
on the earth, in degrees: latitude north or south, longitude east or
west. Stops use them for where each stop stands.

**Layover.** Time a vehicle waits at the end of a trip before its
next trip. See also **Recovery**.

**Live document.** The agency's live feeds joined to the network: the
delays, alerts, and cancellations in force, in the network's terms.
The **rt** capability makes a new one each time the feeds are read.

**Manifest.** A small file that says which documents a deployment is
using right now, by content hash, and gives the agency's time zone.
Every app reads it first and checks it again at every poll.

**Materialized day.** All the trips that run on one date, worked out
from the network and with the day's detours and cancellations
applied. The board and the trip planner both work from it.

**Module.** A WebAssembly program file. A module holds one or more
capabilities and the core.

**Network.** The software's model of the agency's service: routes,
directions, patterns, stops, trips, service days, blocks, fares, and
more. It is built from a GTFS feed and can be exported back to one
unchanged.

**Op.** One kind of request a capability answers, such as
`stop_board` or `plan`. Each op is documented with a worked example
that the build runs to make sure it stays correct.

**Pattern.** One sequence of stops a route serves in one direction.
A route with a short-turn or a branch has more than one pattern.
Agencies may call a pattern a variant.

**Pick.** A version of the schedule that takes effect on a set date,
as in the agency's service change or "pick" periods, a few times a
year. A published pick can be retired before its date, but is never
removed.

**Plan set.** The list of published picks and their start dates.
Each app asks which pick is in force on today's date.

**Poll.** Checking again for new data at a fixed interval. The stop
display polls every 20 seconds, for example. A page that is not
visible does not poll.

**Pointer.** A name in the server's store that points to the current
version of a document, such as the manifest or the edit document.
Moving a pointer is how the store publishes a change. Every move is
logged.

**Record.** One entry in the edit document: one detour or one trip
cancellation.

**Recovery.** The minimum time a vehicle must wait between trips, set
by the agency's work rules or by the shortest wait already scheduled
at that terminal. A wait longer than the recovery has slack.

**Registry.** The published set of modules, each with a record of the
tests it passed and the source code it was built from. caps-run loads
modules from it. It also publishes the schema of each kind of
document. See
[architecture/registry.md](../architecture/registry.md).

**Retired.** The software's form of "deleted". A retired thing stays
in the history, marked as retired, and no longer takes effect.

**Run.** A piece of work for one operator, cut from one or more
blocks. Work on runs is still in progress.

**Schema.** A description of a kind of document: its fields and what
each may hold. Another program can check a document against it, and
read the document without this software. See
[architecture/registry.md](../architecture/registry.md).

**Service combination.** A set of service types that run together on
the same dates, such as "Weekday + School Day". Every date with
service falls in exactly one combination.

**Service type.** A named set of days that trips run on, such as
"Weekday" or "Saturday". GTFS calls it a `service_id`.

**Service worker.** A small script that the browser keeps for an app.
It saves the app on the device so the app can start offline, and it
downloads new builds in the background.

**Static web host.** A service that serves files and runs no program
of yours, such as Cloudflare. The rider apps can be deployed to one.

**Stop request.** A request for work at a stop: a new stop, a
reinstall, a move or removal, a repair, or a temporary stop. It is the
planning team's record of the work, and it keeps the number of the
**Work order** that the work order system opens for it.

**Store.** The server's durable storage for documents, kept in cloud
object storage. It only ever adds files. See
[FORMATS.md](../FORMATS.md).

**Streets document.** The agency's street network, read from a map
file (GeoJSON). The software uses it for walking directions and for
drawing detours along streets.

**TIDES.** A standard format for records of service that actually ran
(arrivals, passenger counts). The plan is for wasm-transit to read it,
to learn how long trips really take. wasm-transit will never write
it.

**TODS.** A standard format, alongside GTFS, for operational
schedule data such as runs and deadheads.

**Trip instance.** One trip on one date. A trip that runs every
weekday has one instance per weekday.

**Unit.** One piece of open work, described in a file in
[docs/units/](../units/README.md). The file is deleted when the work
is done.

**Walk table.** The walking times between nearby stops, along the
streets. The trip planner uses it for transfers.

**Walking group.** A station and the stops or bays that belong to it,
treated as one place for transfers and departure boards. The bays of
a transit center are one walking group.

**Watchlist.** A rider's saved stops, kept on their own device.

**WebAssembly (wasm).** A standard format for programs that browsers
and servers can both run, safely and at near-native speed. The
project's name comes from it.

**Work order.** The job record in the agency's work order system
that crews work from. This app does not hold work orders; a **Stop
request** keeps a work order's number, link, and last known status.

**Work rules.** The agency's rules for scheduling vehicles and
operators, such as minimum recovery time and which routes may share
a vehicle. Every rule is optional.
