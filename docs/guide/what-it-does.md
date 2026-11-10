# What wasm-transit does

wasm-transit turns a transit agency's published schedule and live
data into software for riders and planners. It reads the two standard
formats agencies already publish:

- **GTFS**, the schedule: routes, stops, trips, and the days they run.
- **GTFS-realtime**, the live data: where buses are, how late they
  are, and service alerts.

From these it builds apps. Each app runs in a web browser, does one
job for one kind of user, and is deployed on its own.

## The apps

The set of apps grows over time. New apps are built from the same
parts, so adding one does not change the others. A deployment's start
page lists the apps it offers. The current list, and which apps need a
server, is in the
[architecture reference](../architecture/README.md#each-ui-is-its-own-product).

Some examples of what the apps do:

- **For riders, on their phones:** find stops and addresses, show
  the next departures with live times, plan trips on a map, and keep
  saved stops. Installs like an app and works offline.
- **For a screen at a stop or transit center:** show the next
  departures at one stop, with live times, alerts, and detours. Runs
  for weeks with no one watching it.
- **For service planners:** plan a service change. See how the
  network performs today, who it reaches, and what a change costs.
  Then publish the change for a future date.

## Two ways to deploy it

An agency chooses how to deploy each app.

**Without a server.** A rider app can be exported as a folder of
plain files. Any static web host can serve them, such as
Cloudflare. No program of the agency's runs anywhere. The folder
holds the schedule. Alternatively, the apps can fetch the agency's
GTFS file themselves and check it once a day for a new version.
[DEPLOYMENTS.md](../../DEPLOYMENTS.md) walks through this step by
step.

**With the server.** A small server program adds what only a shared
server can hold:

- detours and trip cancellations entered by staff, shown to riders;
- schedule changes published ahead of their start date;
- address search, through a geocoding service;
- sign-in for staff who make changes;
- a durable, auditable store of every change, kept in cloud object
  storage.

Apps for staff, such as service planning, need the server. Apps for
riders do not. Neither way is a cut-down version of the other. An app that runs
without a server keeps working without one, and the server's
features stay with the server.

## What makes it different

- **Your device does the work.** The schedule and the answers are
  computed in the rider's browser, not on a central server. A rider's
  trips and saved stops stay on their device. The one exception is
  address search: the text a rider types goes to the geocoding
  service, and nothing else does. Doing the work on the device is why
  the rider apps work offline and need no server.
- **The same answer everywhere.** The browser, the server, and a
  command-line tool all run exactly the same code over exactly the
  same data. They give the same answer, byte for byte. You can check
  any answer the app shows by asking the same question another way.
- **Open data, no lock-in.** Everything the software stores is in an
  open format: GTFS, GTFS-realtime, GeoJSON, or plain JSON with a
  published description. You can read it without the software. The
  server's store works with Amazon S3, Azure Blob Storage, or Google
  Cloud Storage, and favors none of them.
- **Nothing is overwritten.** A change creates a new version and keeps
  the old one. "Deleting" marks a thing as retired. This gives a
  complete history for audits, and makes rolling back a matter of
  pointing at an older version.

[Why it is built this way](why-it-is-built-this-way.md) explains each
of these choices and what it costs.

## What it does not do

- **It does not run the dispatcher's day.** Computer-aided dispatch
  and automatic vehicle location (CAD/AVL), workforce management, and
  the agency's scheduling system stay the systems of record for their
  own data. wasm-transit reads what they publish and accepts changes
  from them in a defined way.
- **It does not replace the agency's scheduling system.** That system
  stays the final authority. Changes made here are proposed, reviewed,
  and accepted, and the schedule leaves as a standard GTFS file.
- **It does not publish its own live feed.** It reads the agency's
  GTFS-realtime feeds. Detours and cancellations entered here are
  shown in its own apps, not sent out as a GTFS-realtime feed.

## Where the project stands

Some parts are further along than others, and that is by design:
each app is its own product and can go to production on its own
schedule. What is still to be done is listed in
[docs/units/](../units/README.md), one file per piece of work.

## Read next

[How it works](how-it-works.md) follows one stop display from
start-up to the departures on its screen.
