# Deploying ride-now and the stop display as static sites (Cloudflare Workers)

This guide takes you from a fresh checkout to the rider apps running on
Cloudflare, with no server of your own. You do not need to read the
code.

The two rider apps are **separate deployments**. Each has its own
export, its own Cloudflare Worker, its own address, and its own
schedule data. You can deploy, update, and roll back each one without
touching the other, or deploy only one of them.

1. [What you are deploying](#1-what-you-are-deploying)
2. [What you need](#2-what-you-need)
3. [Build a deployment](#3-build-a-deployment)
4. [Set up the Cloudflare project (once per app)](#4-set-up-the-cloudflare-project-once-per-app)
5. [Deploy](#5-deploy)
6. [Check the deployment](#6-check-the-deployment)
7. [Set up the screens and phones](#7-set-up-the-screens-and-phones)
8. [Publish a new schedule or a new app version](#8-publish-a-new-schedule-or-a-new-app-version)
9. [Roll back](#9-roll-back)
10. [What a static deployment does not do](#10-what-a-static-deployment-does-not-do)
11. [Troubleshooting](#11-troubleshooting)

The project can also run behind its own server ([DEPLOYMENTS-
SERVER.md](DEPLOYMENTS-SERVER.md)). The server adds detours, address
search, and the planning tool (service design), which only works with a
server. Everything here is the server-free option for the two rider
apps. Both options use the same pages and the same file layout, so you
can move between them.

---

## 1. What you are deploying

| App | Who uses it | Deployed with |
|---|---|---|
| **ride-now** | Riders, on their phones: nearby stops, next departures, trip planning on a map, live bus positions. Installable as an app; works offline. | `just export ride-now …` |
| **stop-display** | A screen at a stop (for example a Raspberry Pi 4 in kiosk mode): the next departures at one stop. Works offline. | `just export stop-display …` |

Each runs entirely in the browser. A deployment is a folder of plain
files. For ride-now:

```
out/
  index.html             opens ride-now.html (the deployment's address alone works)
  ride-now.html          the app's page
  sw.js                  the service worker (offline support and updates)
  assets/                scripts and styles (file names include a hash)
  caps/                  the app's WebAssembly modules (names include a hash)
  basemap.pmtiles        the map tiles (too large for Cloudflare: left out, below)
  basemap-assets/        map fonts and icons
  manifest               which schedule is current: a small JSON file
  blobs/sha256:<hash>    the schedule, the street network, and the walking
  blobs/sha256:<hash>.gz   times, each named by the hash of its contents
  live-feeds.json        where the live (GTFS-realtime) feeds are
```

The stop display's folder is the same, with `stop-display.html` instead
of `ride-now.html`, and no map files: it draws no map.

How the pages use it: the page reads `manifest`. It names either the
current schedule's files in `blobs/`, or the agency's GTFS URL for the
page to download itself ([section 3](#3-build-a-deployment) explains
the choice). The page checks every file it downloads against its hash
and refuses one that does not match. An open page re-reads `manifest`
every 20–30 seconds, so a new deployment reaches every open screen and
phone without anyone reloading it.

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
files per deployment. Ride-now is about 800 files (most of them map
fonts); the stop display about 20.

**The agency's data:**

| Input | Required | What it is |
|---|---|---|
| GTFS zip | yes | The agency's published schedule: a file or an `https://` URL. To have the apps follow it without redeploys, a public `https://` URL that sends CORS headers ([section 3](#3-build-a-deployment)). |
| Street layer | no (ride-now only) | A GeoJSON file of street centerlines: the agency's GIS export, TIGER, or OpenStreetMap. Without it, ride-now does not compute walking along streets. The stop display does not use it. |
| Live feed URLs | no | The agency's GTFS-realtime feeds: trip updates, vehicle positions, alerts. Without them, boards show the timetable only. |

## 3. Build a deployment

### Choose how the app gets its schedule

Choose for each app separately.

| | **A. Built in** (`just export`) | **B. Follow the GTFS URL** (`just export-feed-url`) |
|---|---|---|
| A new GTFS zip from the agency | Rebuild and redeploy | Nothing to do: every phone and screen checks the URL once a day and switches to a new zip on its own |
| Who processes the GTFS | The export, once | Every phone and screen, at start-up and when the zip changes |
| Start-up on a device | Fast: the schedule arrives ready to use (C-TRAN: about 0.6 MB) | Slower and heavier: each device downloads the zip and processes it (C-TRAN: about 3 seconds on a desktop, longer on a phone or a Pi 4, and about 190 MB of memory) |
| Walking between stops, for transfers (ride-now) | Yes, with a street layer | No (walking from the rider's location still works with a street layer) |
| What the GTFS URL must do | Nothing: the export reads it | Answer over HTTPS with an `Access-Control-Allow-Origin` header, like the live feeds below |

Choose A when you redeploy anyway when the agency publishes a new
schedule, or when the screens are low-memory devices. Choose B when
nobody should have to redeploy for a new feed. You can switch at any
later deployment.

### Create the live-feeds file

Create `deploy/live-feeds.json` with the agency's GTFS-realtime feeds.
The repository ignores the `deploy/` folder, so nothing in it (feed
addresses, Cloudflare settings) is committed by accident.
Both apps can use the same file. Any of the three may be left out:

```json
{
  "trips": "https://feeds.example.org/gtfs-rt/TripUpdates",
  "vehicles": "https://feeds.example.org/gtfs-rt/VehiclePositions",
  "alerts": "https://feeds.example.org/gtfs-rt/Alerts"
}
```

The browser downloads these feeds directly, so **each feed must be
served over HTTPS and send an `Access-Control-Allow-Origin` header**
(`*`, or your apps' addresses). A feed that does not is skipped, and
the boards show the timetable instead. This is set on the feed's
server, usually by the agency's CAD/AVL vendor.
[Section 11](#11-troubleshooting) shows how to check a feed.

Two more settings on the feed's server matter at scale:

- **A short cache time.** `Cache-Control: max-age=` about the feed's
  refresh interval (for example 15 seconds). Then a CDN in front of the
  feed answers the many screens and phones that ask for the same bytes.
- **Each trip's start date.** Trip updates should give each trip's
  `start_date`. A trip that runs past midnight is then read as part of
  the day it started. Without it, the apps read the trip as part of the
  current service day.

> Do not use the `live-feeds.json` in `web/public/`. It is a development
> copy that sends feeds through a local relay (`"proxy": "/rt"`) that a
> static site does not have.

### Run the export

**Option A** (the schedule built in), one command per app:

```sh
LIVE_FEEDS=deploy/live-feeds.json \
  just export ride-now exports/ride-now/2026-11-10 path/to/gtfs.zip path/to/streets.geojson "Agency Name"

LIVE_FEEDS=deploy/live-feeds.json \
  just export stop-display exports/stop-display/2026-11-10 path/to/gtfs.zip "" "Agency Name"
```

Arguments, in order:

1. The app: `ride-now` or `stop-display`.
2. The output folder. It must not exist yet or be empty. Use a new
   folder for every export; a dated name works well.
3. The GTFS zip (required).
4. The street layer (ride-now; `""` to skip it and still give a name).
5. The publisher's name, shown as the source of the schedule
   (optional).

The command builds the app (a few minutes the first time), imports the
schedule (and the streets), and writes the folder. It ends with a line
like:

```
export: /…/exports/ride-now/2026-11-10 — manifest, 4 documents, the pages from ../web/dist-ui/ride-now
```

If it also prints `live-feeds.json names the relay "/rt"`, you forgot
`LIVE_FEEDS=`.

**Option B** (follow the GTFS URL):

```sh
LIVE_FEEDS=deploy/live-feeds.json \
  just export-feed-url stop-display exports/stop-display/2026-11-10 https://agency.example.org/gtfs.zip "" "Agency Name"
```

The arguments are the same, except the third is the GTFS URL the
devices download. The export does not download it. Check first that it
answers with CORS:

```sh
curl -sI https://agency.example.org/gtfs.zip | grep -iE '^HTTP|access-control-allow-origin'
```

It must print `200` and an `access-control-allow-origin` line. Without
that header the app stays empty: browsers refuse to read the zip.

For C-TRAN's feed and county roads, ride-now's largest file is the
street network at about 23 MB; Cloudflare's limit is 25 MiB per file.
A street layer much larger than C-TRAN's county roads could exceed it.

## 4. Set up the Cloudflare project (once per app)

Each app is its own Cloudflare Worker, with its own folder. The `site/`
folder stays between deployments ([section 8](#8-publish-a-new-schedule-or-a-new-app-version)
explains why):

```
deploy/
  live-feeds.json
  ride-now/
    wrangler.jsonc
    site/
      .assetsignore
  stop-display/
    wrangler.jsonc
    site/
```

**`deploy/ride-now/wrangler.jsonc`**:

```jsonc
{
  "name": "transit-ride-now",
  "compatibility_date": "2026-11-01",
  "assets": {
    "directory": "./site",
    // Serve /ride-now.html as it is. The default, "auto-trailing-slash",
    // redirects it to /ride-now, which breaks offline support.
    "html_handling": "none",
    // A missing file is a 404. The app expects this, for example for
    // the detours file that only a server deployment has.
    "not_found_handling": "none"
  }
  // To use your own domain instead of transit-ride-now.<you>.workers.dev:
  // "routes": [{ "pattern": "ride.example.org", "custom_domain": true }]
}
```

**`deploy/stop-display/wrangler.jsonc`** is the same with
`"name": "transit-stop-display"` (and its own domain, if any).

`name` becomes part of the address: `https://transit-ride-now.<your
subdomain>.workers.dev`. There is no Worker script: the files are
served as they are.

**`deploy/ride-now/site/.assetsignore`**, the files never uploaded:

```
basemap.pmtiles
```

The map tile archive is about 90 MiB, over Cloudflare's 25 MiB limit
per file, and the upload fails with `Asset too large` if it is
included. Without it, the map loads its tiles from
[OpenFreeMap](https://openfreemap.org), a free public tile service with
no key. Phones need internet access to it for the map; departures and
trip planning do not depend on it. The stop display has no map, so it
needs no `.assetsignore`.

**`_headers`** is a file you do not write. Each export writes it into
its folder, with the rest of the files. It tells Cloudflare which files
to keep forever, and which security headers each page sends. It is
different for each app, because each page names its own sources (the
live feeds, the map's tile hosts). Copying an export into `site/`
replaces the old `_headers`, as it should.

What the file says:

- **Files named by their hash never change**, so browsers may keep
  them forever: `/assets/*`, `/caps/*`, and `/blobs/*`. A new version of
  one of them has a new name.
- **Every other file** must be checked with Cloudflare on each use:
  the pages, `manifest`, `sw.js`, `caps/manifest.json`, and
  `live-feeds.json`. Cloudflare does this by default
  (`Cache-Control: public, max-age=0, must-revalidate`), so these files
  need no rule. **Do not add a long cache time for `manifest` or
  `sw.js`**: screens would stop picking up new schedules and new
  versions.
- `manifest` has no file extension, so a rule labels it as JSON. The
  pages read it either way; the label is for people and tools looking at
  it.
- **Security headers.** Every file says it must not be guessed at
  (`X-Content-Type-Options`). The page itself lists the places it may
  load from and connect to, and which phone features it may use. Ride-now
  may ask for the rider's location. The stop display may not.
- **Do not set `Content-Encoding` on the `.gz` files.** They are
  meant to arrive compressed: the app decompresses them itself, which
  keeps the 10 MB schedule out of the device's memory. The app still
  works if something decompresses them in transit.

If you change `live-feeds.json` by hand after the export, run the export
again. The page's security header names the feed hosts the export saw.
A feed on another host would be refused.

**Log in once:**

```sh
npx wrangler login
```

For CI, set `CLOUDFLARE_API_TOKEN` (a token made from the "Edit
Cloudflare Workers" template) and `CLOUDFLARE_ACCOUNT_ID` instead of
logging in.

## 5. Deploy

Copy an app's export into its site folder, then deploy that app:

```sh
rsync -a exports/ride-now/2026-11-10/ deploy/ride-now/site/
cd deploy/ride-now && npx wrangler deploy
```

and, separately, whenever you choose:

```sh
rsync -a exports/stop-display/2026-11-10/ deploy/stop-display/site/
cd deploy/stop-display && npx wrangler deploy
```

**Never use `rsync --delete`, and never empty `site/`**: see
[section 8](#8-publish-a-new-schedule-or-a-new-app-version) for why.
The `.assetsignore` you created stays in place, because an export has
no file with that name. Each export brings its own `_headers`.

Before the first real deployment, `npx wrangler deploy --dry-run`
checks the configuration and the files without uploading anything.

**A second host.** An export is plain static files. You can deploy the
same export folder to a second web host, so that the app stays up when
one host is down. Put both hosts behind the same address, and fail
over in DNS. Do not give the second host its own address: an
installed screen or phone keeps the address it was set up with. Deploy
each export to both hosts, and keep their `site/` folders the same.

## 6. Check the deployment

Replace `HOST` with the app's address, for example
`transit-ride-now.example.workers.dev`.

```sh
# The manifest: names the schedule's files (option A) or the GTFS URL (option B).
curl -s https://HOST/manifest

# Option A: the schedule file it names. Expect 200 and application/gzip,
# with no Content-Encoding line.
NET=$(curl -s https://HOST/manifest | python3 -c 'import json,sys; print(json.load(sys.stdin)["network_hash"])')
curl -sI "https://HOST/blobs/$NET.gz" | grep -iE '^HTTP|content-type|content-encoding|cache-control'

# Option B: run the CORS check from section 3 on the GTFS URL instead.

# The page: expect 200, not a redirect (stop-display.html for the stop display).
curl -sI https://HOST/ride-now.html | head -1

# The page's security headers: expect a content-security-policy and a
# permissions-policy line.
curl -sI https://HOST/ride-now.html | grep -iE 'content-security-policy|permissions-policy'

# The schedule engine arrives compressed: expect a content-encoding line
# (gzip or br). Take a file name from the caps/ folder of the export.
curl -sI -H 'Accept-Encoding: gzip, br' https://HOST/caps/FILE.wasm | grep -i content-encoding
```

If the last check prints nothing, the host does not compress `.wasm`
files. The app still works, but a first visit downloads about 5 MB of
engine instead of about 1.5 MB. Turn on compression for
`application/wasm` in the host's settings.

Then open `https://HOST/` in a browser. It opens the app. For
ride-now, the nearby stops and their next departures should appear
within a few seconds.

Open the browser's developer console while the app loads a stop, its
live departures, and the map. Do this once on a phone and once on a
stop display. A message that a request was blocked by the
`Content-Security-Policy` means the policy does not name a host the
app needs. Check that `live-feeds.json` names that host, and run the
export again.

## 7. Set up the screens and phones

**Stop displays.** First, make sure each screen keeps the right time.
A display works out its departures from the device's clock, so a wrong
clock shows wrong departures. Turn on network time (NTP), and check the
time after the first boot. A Raspberry Pi has no battery to keep its
clock: after a power cut with no network, it starts with the time it
had when it shut down. Also turn off screen blanking and power saving.

Open the stop display's address on the screen. With
no stop chosen, it opens its setup: pick the stop and a layout. The
page's address then holds both, for example
`https://HOST/stop-display.html?stop=…&layout=routes`. Set that address
as the kiosk browser's start page, and the screen reopens on its stop
after a power cut. The three layouts:

| `layout=` | For |
|---|---|
| `center` | A transit center: every route and bay as a list. |
| `route` | A stop one route serves: that route fills the screen. |
| `routes` | A street stop with two or three routes, each a band across the screen. |

A display stores the app the first time it loads. After that it starts
and shows the timetable even without a network connection, and says
the times are scheduled rather than live.

**Phones.** Share ride-now's address. Riders can add it to their home
screen; it then works offline with the last schedule it downloaded.

## 8. Publish a new schedule or a new app version

Each app is updated on its own. The steps below are for one app;
repeat them for the other when you want it updated too.

### A new schedule, built in (option A)

Export into a **new** folder, copy it over the site, and deploy:

```sh
LIVE_FEEDS=deploy/live-feeds.json \
  just export ride-now exports/ride-now/2027-01-15 path/to/new-gtfs.zip path/to/streets.geojson "Agency Name"
rsync -a exports/ride-now/2027-01-15/ deploy/ride-now/site/
cd deploy/ride-now && npx wrangler deploy
```

Within about 30 seconds, every open phone (or screen, for the stop
display) picks up the new schedule without reloading.

The schedule in force is whatever was exported last. There is no
"takes effect on date X" in a static deployment, so deploy a new
schedule when it should start, for example the night before a service
change. (Scheduling picks ahead of time needs the server deployment.)

### A new schedule, following the GTFS URL (option B)

Nothing to deploy. Every open phone and screen checks the URL once a
day and switches to a new zip without reloading. A page opened or
reloaded checks right away, so restarting a screen picks up a new feed
immediately. If the URL does not answer, devices keep the schedule they
have (offline, the last zip they downloaded) and check again an hour
later.

### A new app version (a code update)

Run the same export (`just export` or `just export-feed-url`, the same
app and data) on the updated code, then copy and deploy as above. The
new version downloads in the background. Stop displays switch to it on
their own within a minute or two. Phones show "a new version is ready"
and switch at the next launch, or when the rider accepts.

### Why `site/` keeps old files

Each deployment replaces everything Cloudflare serves for that app
with the contents of its `site/`. The steps above keep the previous
version's files in it, for two reasons:

- A page that read the old `manifest` moments before a deployment
  still finds the files it names.
- A page still running the previous app version can still load its
  scripts.

Ride-now's folder grows by roughly 40 MB of data per schedule update
(the stop display's by about 10 MB), plus each app version's scripts.
Wrangler uploads only files Cloudflare does not already have, so a
deployment stays quick.

**Trimming it.** When a `site/` gets large or nears the file limit
(20,000 files on the free plan), rebuild it from the latest export
outside service hours, for example for ride-now:

```sh
mv deploy/ride-now/site deploy/ride-now/site.old
mkdir deploy/ride-now/site
cp deploy/ride-now/site.old/.assetsignore deploy/ride-now/site/
rsync -a exports/ride-now/<latest>/ deploy/ride-now/site/
```

Open pages recover on their next refresh.

## 9. Roll back

Cloudflare keeps each app's previous deployments:

```sh
cd deploy/ride-now
npx wrangler deployments list      # recent deployments and their version IDs
npx wrangler rollback <version-id> # serve that version again
```

Open pages follow within about 30 seconds: the restored `manifest`
names the earlier schedule, and its files are still on Cloudflare. Also
restore `site/manifest` from that export folder
(`cp exports/ride-now/<that-export>/manifest deploy/ride-now/site/`).
Otherwise the next deployment would bring back the schedule you rolled
away from. Rolling back one app does not touch the other.

## 10. What a static deployment does not do

These need the server deployment ([DEPLOYMENTS-SERVER.md](DEPLOYMENTS-SERVER.md)):

| Feature | In a static deployment |
|---|---|
| Detours and trip cancellations entered by staff | Not shown. The pages ask for a detours file, get a 404, and carry on. Detours in the agency's own GTFS-realtime alerts still show. |
| Address search | Off. Riders search by stop name, use their location, or tap the map. |
| The planning tool (service design) | Not part of a static deployment: it needs the server. |
| Schedules that take effect on a future date | Not available. The last export, or the zip at the GTFS URL (option B), is the schedule in force. A GTFS zip that already carries the next service change's dates covers this for most agencies. |
| Reports from screens (which version each runs, whether it is healthy) | Not available. |

## 11. Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `wrangler deploy` fails with `Asset too large` | A file is over 25 MiB. If it is `basemap.pmtiles`, `deploy/ride-now/site/.assetsignore` is missing. If it is a file under `blobs/`, the street layer is too large: export a smaller area. |
| `/ride-now.html` (or `/stop-display.html`) redirects to `/ride-now` | `html_handling` is not `"none"` in that app's `wrangler.jsonc`. |
| Boards show only timetable times, never live predictions or bus positions | A live feed is not readable from the browser. In the browser's developer console, a CORS or mixed-content error names it. Check it with `curl -sI <feed URL>`: it must be `https://` and include `access-control-allow-origin`. The fix is on the feed's server. Also check `https://HOST/live-feeds.json` is your file, not the development copy. |
| The map is grey, but stops and departures work | The phone cannot reach OpenFreeMap (`tiles.openfreemap.org`), for example behind a firewall. Allow it. |
| A screen keeps showing an old schedule | Check `https://HOST/manifest` names the new schedule. If it does, check that the `_headers` file in `site/` does not give `/manifest` or `/sw.js` a long cache time (only a rule added by hand would). A screen that is offline keeps its last schedule until it reconnects. |
| Option B: the app stays empty or keeps saying it is loading | The GTFS URL is not readable from the browser. Check it with the `curl` command in [section 3](#3-build-a-deployment); it must send `access-control-allow-origin`. The developer console names the error. |
| Option B: a new feed has not appeared | Devices check once a day. Reload the page, or restart the screen, to check now. Check `https://HOST/manifest` names the right `feed_url`. |
| The export stops with "is not empty" | Exports go to a new folder every time. Pick a new name. |
| The export says `no generated UI by that id` | The first argument is the app: `ride-now` or `stop-display`. |
| Files named `sha256:…` fail to copy | The machine is Windows. Run the export and the deployment on Linux, macOS, or WSL. |

For how this works inside the project, see
`docs/architecture/static-deployment.md`.

## Headers every host must send

Cloudflare Workers and Netlify read the `_headers` file in each export,
so on those hosts you do nothing. Any other static host (a bucket behind
a CDN, nginx, Apache) must send the same headers. Read them from the
`_headers` file in the export, which is plain text: a path, then one
header on each indented line. These are the rules.

| Files | Header | Why |
|---|---|---|
| `/assets/*`, `/caps/*`, `/blobs/*` | `Cache-Control: public, max-age=31536000, immutable` | Their names carry a hash, so they never change. Without this rule, a first visit downloads every large file twice (33 MB for ride-now on a bare host), because the app's offline cache cannot reuse an uncached answer. |
| Every other file, above all `manifest`, `sw.js`, and the pages | Revalidate on each use (`no-cache`, or the host's default). Never a long cache time. | Screens must pick up a new schedule and a new version. |
| `/manifest` | `Content-Type: application/json` | The file has no extension. |
| Every file | `X-Content-Type-Options: nosniff` and `Referrer-Policy: strict-origin-when-cross-origin` | A browser must not guess a file's type, and must not tell other sites which page the rider came from. |
| `/ride-now.html`, `/stop-display.html` | `Content-Security-Policy` and `Permissions-Policy`, exactly as the export wrote them | The page may load and connect only to the hosts it needs. |
| Text and `.wasm` files | Compress with gzip (or Brotli) | Ride-now's schedule engine is 5 MB, and about 1.5 MB compressed. [Section 6](#6-check-the-deployment) checks it. |
| `/blobs/*.gz` | No `Content-Encoding`, and no second compression | The app decompresses these files itself. |

Do not put a policy on `index.html`. It runs one small script that
forwards to the page, and the policy would block it.

If you change the feed hosts, run the export again, so the policy names
the new hosts. A page cannot read a feed its policy does not name.
