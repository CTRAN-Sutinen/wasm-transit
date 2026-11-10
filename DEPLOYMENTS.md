# Deploying the rider apps as a static site (Cloudflare Workers)

This guide takes you from a fresh checkout to the rider apps running on
Cloudflare, with no server of your own. You do not need to read the
code. It covers:

1. [What you are deploying](#1-what-you-are-deploying)
2. [What you need](#2-what-you-need)
3. [Build the site](#3-build-the-site)
4. [Set up the Cloudflare project (once)](#4-set-up-the-cloudflare-project-once)
5. [Deploy](#5-deploy)
6. [Check the deployment](#6-check-the-deployment)
7. [Set up the screens and phones](#7-set-up-the-screens-and-phones)
8. [Publish a new schedule or a new app version](#8-publish-a-new-schedule-or-a-new-app-version)
9. [Roll back](#9-roll-back)
10. [What a static deployment does not do](#10-what-a-static-deployment-does-not-do)
11. [Troubleshooting](#11-troubleshooting)

The project can also run behind its own server (`just serve`; see
`README.md`), which adds detours, address search, and publishing from
the planning tool. Everything here is the server-free option. Both use
the same pages and the same file layout, so you can move between them.

---

## 1. What you are deploying

Two web apps for riders, plus a planning tool:

| Page | Who uses it |
|---|---|
| `ride-now.html` | Riders, on their phones: nearby stops, next departures, trip planning, live bus positions. Installable as an app; works offline. |
| `stop-display.html` | A screen at a stop (for example a Raspberry Pi 4 in kiosk mode): the next departures at one stop. Works offline. |
| `service-design.html` | Planners. **Limited without a server** — see [section 10](#10-what-a-static-deployment-does-not-do). |
| `index.html` | A picker linking to the pages above. |

Everything runs in the browser. The site is a folder of plain files:

```
out/
  index.html, ride-now.html, stop-display.html, service-design.html
  sw.js                  the service worker (offline support and updates)
  assets/                scripts and styles (file names include a hash)
  caps/                  the app's WebAssembly modules (names include a hash)
  basemap.pmtiles        the map tiles (too large for Cloudflare — excluded below)
  basemap-assets/        map fonts and icons
  manifest               which schedule is current: a small JSON file
  blobs/sha256:<hash>    the schedule, the street network, and the walking
  blobs/sha256:<hash>.gz   times, each named by the hash of its contents
  live-feeds.json        where the live (GTFS-realtime) feeds are
```

How the pages use it: every page reads `manifest`, which names the
current schedule's files in `blobs/`. Each page checks every file it
downloads against its hash and refuses one that does not match. An
open page re-reads `manifest` every 20–30 seconds, so publishing a new
schedule reaches every open screen and phone without anyone reloading
it.

## 2. What you need

**A Linux or macOS machine (or WSL on Windows).** The data files are
named `sha256:<hash>`, and Windows file systems cannot hold a `:` in a
file name. Build and deploy from the same machine or CI runner.

**Tools**, installed once:

- [rustup](https://rustup.rs). The repository pins its Rust version
  (`rust-toolchain.toml`); rustup installs the right one the first time
  you build.
- Go, at least the version in `server/go.mod`.
- Node.js and npm (a current LTS release).
- [`just`](https://github.com/casey/just), the task runner.

Then, in the repository:

```sh
cd web && npm install && cd ..
```

**A Cloudflare account.** The free plan is enough: it allows 20,000
files per deployment, and the site is about 1,100.

**The agency's data:**

| Input | Required | What it is |
|---|---|---|
| GTFS zip | yes | The agency's published schedule (a path or an `https://` URL). |
| Street layer | no | A GeoJSON file of street centerlines: the agency's GIS export, TIGER, or OpenStreetMap. Without it, the apps do not compute walking along streets. |
| Live feed URLs | no | The agency's GTFS-realtime feeds: trip updates, vehicle positions, alerts. Without them, boards show the timetable only. |

## 3. Build the site

### Create the live-feeds file

Create `deploy/live-feeds.json` with the agency's GTFS-realtime feeds.
Any of the three may be left out:

```json
{
  "trips": "https://feeds.example.org/gtfs-rt/TripUpdates",
  "vehicles": "https://feeds.example.org/gtfs-rt/VehiclePositions",
  "alerts": "https://feeds.example.org/gtfs-rt/Alerts"
}
```

The browser downloads these feeds directly, so **each feed must be
served over HTTPS and send an `Access-Control-Allow-Origin` header**
(`*`, or your site's address). A feed that does not is silently
skipped, and the boards show the timetable instead. This is set on
the feed's server, usually by the agency's CAD/AVL vendor.
[Section 11](#11-troubleshooting) shows how to check a feed.

> Do not use the `live-feeds.json` in `web/public/`. It is a development
> copy that sends feeds through a local relay (`"proxy": "/rt"`) that a
> static site does not have.

### Run the export

```sh
LIVE_FEEDS=deploy/live-feeds.json \
  just export exports/2026-11-10 path/to/gtfs.zip path/to/streets.geojson "Agency Name"
```

Arguments, in order:

1. The output folder. It must not exist yet or be empty. Use a new
   folder for every export; a dated name works well.
2. The GTFS zip (required).
3. The street layer (optional; use `""` to skip it and still give a
   name).
4. The publisher's name, shown as the source of the schedule
   (optional).

The command builds the app (a few minutes the first time), imports the
schedule and the streets, and writes the site. It ends with a line like:

```
export: /…/exports/2026-11-10 — manifest, 4 documents, the pages from ../web/dist
```

If it also prints `live-feeds.json names the relay "/rt"`, you forgot
`LIVE_FEEDS=`.

For C-TRAN's feed and county roads, the export holds about 1,100
files. The largest is the street network at about 23 MB; Cloudflare's
limit is 25 MiB per file. A street layer much larger than C-TRAN's
county roads could exceed it.

## 4. Set up the Cloudflare project (once)

Make a folder that holds the Cloudflare configuration and the site
files. The site folder stays between deployments ([section 8](#8-publish-a-new-schedule-or-a-new-app-version)
explains why):

```
deploy/
  wrangler.jsonc
  live-feeds.json
  site/
    .assetsignore
    _headers
```

**`deploy/wrangler.jsonc`**:

```jsonc
{
  "name": "transit-rider",
  "compatibility_date": "2026-11-01",
  "assets": {
    "directory": "./site",
    // Serve /ride-now.html as it is. The default, "auto-trailing-slash",
    // redirects it to /ride-now, which breaks offline support.
    "html_handling": "none",
    // A missing file is a 404. The apps expect this, for example for
    // the detours file that only a server deployment has.
    "not_found_handling": "none"
  }
  // To use your own domain instead of transit-rider.<you>.workers.dev:
  // "routes": [{ "pattern": "rider.example.org", "custom_domain": true }]
}
```

`name` becomes part of the address: `https://transit-rider.<your
subdomain>.workers.dev`. There is no Worker script: the files are
served as they are.

**`deploy/site/.assetsignore`**, the files never uploaded:

```
basemap.pmtiles
```

The map tile archive is about 90 MiB, over Cloudflare's 25 MiB limit
per file, and the upload fails with `Asset too large` if it is
included. Without it, the map loads its tiles from
[OpenFreeMap](https://openfreemap.org), a free public tile service with
no key. Phones and screens need internet access to it for the map;
departures, boards, and trip planning do not depend on it.

**`deploy/site/_headers`**, the caching rules:

```
/assets/*
  Cache-Control: public, max-age=31536000, immutable
/caps/*
  Cache-Control: public, max-age=31536000, immutable
/blobs/*
  Cache-Control: public, max-age=31536000, immutable
/manifest
  Content-Type: application/json
```

Why these rules:

- **Files named by their hash never change**, so browsers may keep
  them forever. A new version of one of them has a new name.
- **Every other file** must be checked with Cloudflare on each use:
  the pages, `manifest`, `sw.js`, `caps/manifest.json`, and
  `live-feeds.json`. Cloudflare does this by default
  (`Cache-Control: public, max-age=0, must-revalidate`), so these
  files need no rule. **Do not add a long cache time for `manifest`
  or `sw.js`**: screens would stop picking up new schedules and new
  versions.
- `manifest` has no file extension, so the last rule labels it as
  JSON. The pages read it either way; the label is for people and
  tools looking at it.
- **Do not set `Content-Encoding` on the `.gz` files.** They are
  meant to arrive compressed: the app decompresses them itself, which
  keeps the 10 MB schedule out of the phone's memory. The app still
  works if something decompresses them in transit.

**Log in once:**

```sh
cd deploy && npx wrangler login
```

For CI, set `CLOUDFLARE_API_TOKEN` (a token made from the "Edit
Cloudflare Workers" template) and `CLOUDFLARE_ACCOUNT_ID` instead of
logging in.

## 5. Deploy

Copy the export into the site folder, then deploy:

```sh
rsync -a exports/2026-11-10/ deploy/site/
cd deploy && npx wrangler deploy
```

**Never use `rsync --delete`, and never empty `site/`**: see
[section 8](#8-publish-a-new-schedule-or-a-new-app-version) for why.
The `.assetsignore` and `_headers` you created stay in place, because
the export has no files with those names.

Before the first real deployment, `npx wrangler deploy --dry-run`
checks the configuration and the files without uploading anything.

## 6. Check the deployment

Replace `HOST` with your address, for example
`transit-rider.example.workers.dev`.

```sh
# The manifest: names the schedule's files.
curl -s https://HOST/manifest

# The schedule file it names: expect 200 and application/gzip,
# with no Content-Encoding line.
NET=$(curl -s https://HOST/manifest | python3 -c 'import json,sys; print(json.load(sys.stdin)["network_hash"])')
curl -sI "https://HOST/blobs/$NET.gz" | grep -iE '^HTTP|content-type|content-encoding|cache-control'

# A page: expect 200, not a redirect.
curl -sI https://HOST/ride-now.html | head -1
```

Then open `https://HOST/ride-now.html` in a browser. The nearby stops
and their next departures should appear within a few seconds.

## 7. Set up the screens and phones

**Stop displays.** Open `https://HOST/stop-display.html` on the
screen. With no stop chosen, it opens its setup: pick the stop and a
layout. The page's address then holds both, for example
`https://HOST/stop-display.html?stop=…&layout=routes`. Set that
address as the kiosk browser's start page, and the screen reopens on
its stop after a power cut. The three layouts:

| `layout=` | For |
|---|---|
| `center` | A transit center: every route and bay as a list. |
| `route` | A stop one route serves: that route fills the screen. |
| `routes` | A street stop with two or three routes, each a band across the screen. |

A display stores the app the first time it loads. After that it starts
and shows the timetable even without a network connection, and says
the times are scheduled rather than live.

**Phones.** Share `https://HOST/ride-now.html`. Riders can add it to
their home screen; it then works offline with the last schedule it
downloaded.

## 8. Publish a new schedule or a new app version

### A new schedule (a new GTFS zip)

Export into a **new** folder, copy it over the site, and deploy:

```sh
LIVE_FEEDS=deploy/live-feeds.json \
  just export exports/2027-01-15 path/to/new-gtfs.zip path/to/streets.geojson "Agency Name"
rsync -a exports/2027-01-15/ deploy/site/
cd deploy && npx wrangler deploy
```

Within about 30 seconds, every open screen and phone picks up the new
schedule without reloading.

The schedule in force is whatever was exported last. There is no
"takes effect on date X" in a static deployment, so deploy a new
schedule when it should start, for example the night before a service
change. (Scheduling picks ahead of time needs the server deployment.)

### A new app version (a code update)

Run the same export with the same data on the updated code, then copy
and deploy as above. The new version downloads in the background.
Stop displays switch to it on their own within a minute or two.
Phones show "a new version is ready" and switch at the next launch, or
when the rider accepts.

### Why `site/` keeps old files

Each deployment replaces everything Cloudflare serves with the
contents of `site/`. The rules above keep the previous version's files
in it, for two reasons:

- A page that read the old `manifest` moments before a deployment
  still finds the files it names.
- A page still running the previous app version can still load its
  scripts.

The folder grows by roughly 40 MB of data per schedule update, plus
each app version's scripts. Wrangler uploads only files Cloudflare
does not already have, so a deployment stays quick.

**Trimming it.** When `site/` gets large or nears the file limit
(20,000 files on the free plan), rebuild it from the latest export
outside service hours:

```sh
mv deploy/site deploy/site.old
mkdir deploy/site
cp deploy/site.old/.assetsignore deploy/site.old/_headers deploy/site/
rsync -a exports/<latest>/ deploy/site/
```

Open pages recover on their next refresh.

## 9. Roll back

Cloudflare keeps previous deployments:

```sh
cd deploy
npx wrangler deployments list      # recent deployments and their version IDs
npx wrangler rollback <version-id> # serve that version again
```

Open pages follow within about 30 seconds: the restored `manifest`
names the earlier schedule, and its files are still on Cloudflare.
Also restore `deploy/site/manifest` from that export folder
(`cp exports/<that-export>/manifest deploy/site/`). Otherwise the next
deployment would bring back the schedule you rolled away from.

## 10. What a static deployment does not do

These need the server deployment (`just serve`; `README.md`):

| Feature | In a static deployment |
|---|---|
| Detours and trip cancellations entered by staff | Not shown. The pages ask for a detours file, get a 404, and carry on. Detours in the agency's own GTFS-realtime alerts still show. |
| Address search | Off. Riders search by stop name, use their location, or tap the map. |
| Planning tool (`service-design.html`) | Opens, but cannot load the deployed schedule or publish one. Planners load a GTFS zip by hand. |
| Schedules that take effect on a future date | Not available. The last export is the schedule in force. |
| Reports from screens (which version each runs, whether it is healthy) | Not available. |

## 11. Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `wrangler deploy` fails with `Asset too large` | A file is over 25 MiB. If it is `basemap.pmtiles`, `site/.assetsignore` is missing or not in `site/`. If it is a file under `blobs/`, the street layer is too large: export a smaller area. |
| `/ride-now.html` redirects to `/ride-now` | `html_handling` is not `"none"` in `wrangler.jsonc`. |
| Boards show only timetable times, never live predictions or bus positions | A live feed is not readable from the browser. In the browser's developer console, a CORS or mixed-content error names it. Check it with `curl -sI <feed URL>`: it must be `https://` and include `access-control-allow-origin`. The fix is on the feed's server. Also check `https://HOST/live-feeds.json` is your file, not the development copy. |
| The map is grey, but stops and departures work | The device cannot reach OpenFreeMap (`tiles.openfreemap.org`), for example behind a firewall. Allow it. |
| A screen keeps showing an old schedule | Check `https://HOST/manifest` names the new schedule. If it does, check `_headers` does not give `/manifest` or `/sw.js` a long cache time. A screen that is offline keeps its last schedule until it reconnects. |
| The export stops with "is not empty" | Exports go to a new folder every time. Pick a new name. |
| Files named `sha256:…` fail to copy | The machine is Windows. Run the export and the deployment on Linux, macOS, or WSL. |

For how this works inside the project, see `docs/ARCHITECTURE.md`
(server section, "Static deployment").
