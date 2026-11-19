# Running the server

This guide takes you from a fresh checkout to the wasm-transit server
running in production. It is written for a system administrator. You
do not need to read the code.

The server is optional. The rider apps can run without one
([DEPLOYMENTS.md](DEPLOYMENTS.md)). Run the server when the agency
wants what only a shared server can hold:

- detours and trip cancellations entered by staff, shown to riders;
- schedule changes published ahead of their start date;
- the planning app (service design);
- address search, through a geocoder;
- sign-in for staff who make changes;
- a complete, auditable history of every change.

1. [What you are deploying](#1-what-you-are-deploying)
2. [What you need](#2-what-you-need)
3. [Build the image](#3-build-the-image)
4. [Prepare the bucket](#4-prepare-the-bucket)
5. [Run it](#5-run-it)
6. [Turn on sign-in](#6-turn-on-sign-in)
7. [Run it behind a load balancer](#7-run-it-behind-a-load-balancer)
8. [Publish a new schedule](#8-publish-a-new-schedule)
9. [Update or roll back the app](#9-update-or-roll-back-the-app)
10. [Give each app its own address](#10-give-each-app-its-own-address)
11. [Look after the bucket](#11-look-after-the-bucket)
12. [Troubleshooting](#12-troubleshooting)

---

## 1. What you are deploying

One container image. It holds the server program, the app pages, and
everything else the server needs. Nothing inside the container needs
to be kept: all the data lives in a cloud storage **bucket**.

Because of that, any copy of the container can be replaced by a new
one pointed at the same bucket. You can run two or more copies behind
a load balancer for high availability.

```
 phones, stop displays,          load balancer           container(s)          bucket
 planners' browsers      ──►     (TLS, health     ──►    the server     ──►    S3, Azure Blob,
                                  checks)                                       or Google Cloud
                                                                                Storage
```

If the server goes down, the rider apps keep showing the schedule
they last loaded. Staff cannot enter detours until it is back.

## 2. What you need

- **A container runtime**: Kubernetes, a cloud container service, or
  Podman or Docker on a virtual machine.
- **A bucket** in Amazon S3 (or an S3-compatible service), Azure Blob
  Storage, or Google Cloud Storage.
- **The agency's GTFS file**, at a web address the server can reach.
- **Optional:** a street map file (GeoJSON) for walking directions.
- **Optional:** a sign-in provider that supports OpenID Connect
  (Microsoft Entra ID, Okta, Google, Keycloak, and others), if staff
  will make changes.
- **To build the image:** a checkout of this repository, with the
  tools in [README.md](README.md), and
  [ko](https://ko.build/install/).

## 3. Build the image

From the repository:

```
just server-image
```

This writes `target/server-image.tar`. Load it with
`podman load -i target/server-image.tar` (or `docker load`).

To push the image straight to your registry instead, name the
registry and a tag:

```
just server-image ghcr.io/your-agency/transit-server 2026-11-10
```

This builds for both Intel/AMD (`linux/amd64`) and ARM
(`linux/arm64`) machines. Log in to the registry first.

The image is about 160 MB. Most of that is the map tiles the apps
draw.

## 4. Prepare the bucket

1. Create a bucket (on Azure, a container). Use one bucket per
   deployment.
2. Give the server credentials that can **read, list, and create**
   objects. The server never deletes anything, so it does not need
   delete permission. Leave it out.
3. Keep a second set of credentials, **with** delete permission, for
   the clean-up tool only ([section 11](#11-look-after-the-bucket)).

How the server finds its credentials:

| Provider | Store address | Credentials |
|---|---|---|
| Amazon S3 | `s3://bucket/prefix?region=us-west-2` | The usual AWS sources: `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY`, or the role of the machine or pod |
| S3-compatible | `s3://bucket/prefix?endpoint=https://storage.example.com` | As for Amazon S3 |
| Azure Blob Storage | `azblob://container/prefix` | `AZURE_STORAGE_ACCOUNT`, plus `AZURE_STORAGE_KEY`, or `AZURE_STORAGE_CONNECTION_STRING`, or a managed identity |
| Google Cloud Storage | `gs://bucket/prefix` | The usual Google sources: `GOOGLE_APPLICATION_CREDENTIALS`, or the service account of the machine or pod |

If the agency must keep records for a set time, you can turn on the
provider's lock that stops anything being deleted for that time (S3
Object Lock, Azure immutability policies, or a GCS bucket lock). The
server works with it.

Choose the provider your agency already uses. The server works the same
on each. Before you go live, check your provider with the store's own
test suite. Run it from a checkout of the repository, against a
separate prefix that the server never uses, with credentials that can
delete (the suite cleans up after itself):

```
just store-contract 's3://agency-transit/contract-check?region=us-west-2'
```

It must end with `ok`. Run it again if you move to another provider or
region.

## 5. Run it

A minimal production command, with Podman:

```
podman run -d --name transit-server -p 8080:8080 \
  -e AWS_ACCESS_KEY_ID=… -e AWS_SECRET_ACCESS_KEY=… \
  localhost/transit-server:latest \
  -store 's3://agency-transit/prod?region=us-west-2' \
  -feed https://agency.example.com/gtfs.zip \
  -publisher "Example Transit"
```

The server listens on port 8080. Put TLS (HTTPS) in front of it at
the load balancer.

**Memory.** With C-TRAN's feed and its county roads street layer
(99 MB), the server needs about 5 GB while it imports them at startup,
and about 0.7 GB after that. Give the container at least 6 GB, and set
`GOMEMLIMIT` to about 1 GB less than the container's limit (for
example `-e GOMEMLIMIT=5GiB`). Without it, the server keeps memory it
no longer uses for several minutes, and appears to need about 3 GB at
rest. Without a street layer, about 1.5 GB is enough.

The options you are most likely to use:

| Option | What it does |
|---|---|
| `-store` | The bucket ([section 4](#4-prepare-the-bucket)). Always set it in production. |
| `-feed` | Where the GTFS file is: a web address, or `bucket:<name>` for the object `inputs/<name>` in the bucket. |
| `-streets` | Where the street map file is, in the same forms. Turns on walking directions. |
| `-publisher` | The agency's name, shown as the source of the schedule. |
| `-geocoder` | The address of a Pelias geocoder. Turns on address search. |
| `-oidc-…`, `-policy` | Sign-in ([section 6](#6-turn-on-sign-in)). |
| `-drain` | How long to wait before stopping ([section 7](#7-run-it-behind-a-load-balancer)). The default is 5 seconds. |

`podman run localhost/transit-server:latest -help` lists every option.

**Keep the settings in a file.** Instead of a long command, put the
options in a settings file and give the server `-config <file>`. Each
line is an option's name without the dash. A `[oidc]` heading saves
writing `oidc-` before each sign-in option. Lines starting with `#` are
comments.

```
# /etc/transit/server.toml
store = "s3://agency-transit/prod?region=us-west-2"
feed = "https://agency.example.com/gtfs.zip"
publisher = "Example Transit"

[oidc]
issuer = "https://login.example.com/"
audience = "transit-server"
jwks-url = "https://login.example.com/keys"
```

```
podman run -d --name transit-server -p 8080:8080 \
  -e AWS_ACCESS_KEY_ID=… -e AWS_SECRET_ACCESS_KEY=… \
  -v /etc/transit:/etc/transit:ro \
  localhost/transit-server:latest -config /etc/transit/server.toml
```

Keep passwords and keys out of the file: the bucket's credentials still
come from the environment or the platform's identity, as above. An
option given on the command line wins over the file, which is handy for
a one-off change. If the file has a name the server does not know, it
stops at once and the log names the file and the line's name. A file
path written in the file is read from the file's own folder.

**Watch it.** The server writes its log to standard output, one line
for each event. Send it to the platform's log store, and alert on
these:

- `/readyz` answers anything other than `ready` for more than 5
  minutes ([section 7](#7-run-it-behind-a-load-balancer) says what each
  answer means);
- a log line that starts with `DEV STORE` or `auth OFF` in production;
- a log line that starts with `authz deny`, when it repeats for one
  person (they lack a role, or someone is trying their luck);
- the container restarting.

**Check it.** Open `https://<your address>/readyz`. It should say
`{"status":"ready","store":"ok"}`. Then open the address itself: the
start page lists the apps.

**If the log says `DEV STORE`,** the server is running without a
bucket. Edits are kept in memory and lost when it stops. Add `-store`.

## 6. Turn on sign-in

**Do not let the internet reach the server until sign-in is on.**
Without sign-in, anyone who can reach the server can change anything:
publish a schedule, change the stops record, enter detours, and roll
the apps back to an earlier version. The log says `auth OFF` at
startup. Until sign-in is on, keep the server on a private network that
only staff can reach.

1. Register the server with your sign-in provider. Note the
   **issuer**, the **audience** (client ID), and the address of its
   **signing keys** (JWKS). Set up the provider to put each user's
   roles in the sign-in token, in a claim named `roles`.
2. Write a policy file that says which role may change what. For
   example:

   ```json
   {"kind": "policy", "schema_version": 1, "rules": [
     {"name": "dispatch-edits", "roles": ["dispatcher"], "actions": ["write"], "pointers": ["edits"]},
     {"name": "planning",       "roles": ["planner"],    "actions": ["write"], "pointers": ["plans", "stops", "stop_requests"]},
     {"name": "releases",       "roles": ["admin"],      "actions": ["write"], "pointers": ["build"]},
     {"name": "audit",          "roles": ["auditor"],    "actions": ["audit"], "pointers": ["edits"]}
   ]}
   ```

   `edits` is detours and cancellations, `plans` is published schedule
   changes, `stops` is the stops record (and its surveys),
   `stop_requests` is the requests for work at stops, and `build` is
   the app version
   ([section 9](#9-update-or-roll-back-the-app)). Everyone can read.
   The `audit` action on `edits` lets a role read the history of
   detours and cancellations, with who made each change, and undo a
   change ([section 11](#11-look-after-the-bucket)).
3. Add the options:

   ```
   -oidc-issuer https://login.example.com/ \
   -oidc-audience transit-server \
   -oidc-jwks-url https://login.example.com/keys \
   -policy bucket:policy.json
   ```

   If the roles are in a claim with another name, add
   `-oidc-roles-claim <name>`.

   A stop survey records who did it, by the name in the sign-in token's
   `name` claim (in Entra, the person's display name). To record
   another claim, add `-oidc-name-claim <name>` — for example
   `preferred_username` or `upn` for the user's UPN. A token without
   that claim is recorded by its user id (`sub`).

The policy file is read the first time the server starts on a bucket.
After that the bucket's copy is the policy, and every change to it is
logged.

The log of every change names the person who made it. When the server
takes in a new schedule feed, it records how the feed's stops differ
from the stops record; the log names that change `import:feed`.

4. Let the app pages sign in. Register the pages as a second app with
   your provider, of the "single-page application" kind (in Entra:
   Authentication, then Add a platform, then Single-page application).
   Add each page's address as a redirect address:
   `https://<your server>/stops.html`,
   `https://<your server>/service-change.html`, and
   `https://<your server>/service-design.html`. When an app has its own
   address ([section 10](#10-give-each-app-its-own-address)), add that
   address instead, for example `https://stops.agency.example/stops.html`. Give the pages' app
   permission to ask for a token for the server's app. Then add:

   ```
   -oidc-client-id <the pages' app's client ID> \
   -oidc-scope "openid profile offline_access api://<server app>/<scope>"
   ```

   The pages then show a **Sign in** button. A person signs in once per
   browser tab; reading never needs it. Without `-oidc-client-id`, the
   pages cannot sign in and their changes are refused; tools that send
   their own sign-in token still work.

### Keep photos and drawings

Stop surveys and requests can carry photos and site drawings. They are
kept in a bucket of their own, apart from the store: create one (any
provider the store supports) and add `-attachments s3://<bucket>` (or
`gs://…`, `azblob://…`). Only people who are signed in can see them.
To let a role add them, give it write on `attachments` in the policy.
Without `-attachments`, the pages offer no photos. The store's clean-up
tool never touches this bucket.

### Connect the work order system

Stop requests keep the number of the work order your work order system
opens. Until a connection to that system is set up, staff type the
number in by hand on the request, and nothing else is needed. The
option `-work-orders stub:` turns on a stand-in that makes up work
order numbers, for training and demonstrations only. A real connection
is added when the system's API is wired in.

## 7. Run it behind a load balancer

Point the load balancer's health checks at these two addresses:

| Address | Meaning | Use it as |
|---|---|---|
| `/healthz` | The program is running. | The liveness check. If it fails, restart the container. |
| `/readyz` | This copy should get traffic. | The readiness check. If it fails, send traffic elsewhere. |

`/readyz` gives one of four answers:

| Answer | Code | What it means |
|---|---|---|
| `ready` | 200 | All is well. |
| `degraded` | 200 | The bucket is not answering. The server still serves what it has, but refuses changes until the bucket is back. Every copy shares the bucket, so taking them out of service would not help. |
| `unavailable` | 503 | The bucket is not answering and this copy has nothing saved to serve. |
| `draining` | 503 | This copy is stopping. |

**Stopping.** When the container is told to stop (SIGTERM), the
server answers `draining` for 5 seconds, so the load balancer stops
sending it traffic. Then it stops taking new connections, and gives
the requests already running up to 30 seconds to finish. Give the
container at least 40 seconds to stop before it is killed (in
Kubernetes, `terminationGracePeriodSeconds: 40`). If your load balancer
checks less often than every 5 seconds, raise `-drain` to match.

**More than one copy.** Run as many copies as you like on the same
bucket and the same options. A change made through one copy reaches
the others within 2 seconds.

**Compression and headers.** The server does both itself. The load
balancer needs no rule for them.

- The server compresses text and `.wasm` files (gzip) for a browser that
  asks. Ride-now's schedule engine goes from 5 MB to about 1.5 MB. It
  never compresses the `.gz` schedule documents again. A load balancer
  that also compresses is fine. It must not remove `Vary` or
  `Content-Encoding`, and it must not compress the `.gz` documents.
- Every file carries `X-Content-Type-Options: nosniff` and a referrer
  policy. The two rider pages (`/ride-now.html`, `/stop-display.html`)
  also carry a `Content-Security-Policy` and a `Permissions-Policy`.
  The policy lists the hosts in the deployment's `live-feeds.json`. If
  you change the live feeds, the next page load carries the new list.
  Do not add a different `Content-Security-Policy` at the load balancer:
  the browser would apply both, and the stricter one would win.
- To check: `curl -sI https://HOST/ride-now.html | grep -iE
  'content-security-policy|permissions-policy'`, and `curl -sI -H
  'Accept-Encoding: gzip' https://HOST/caps/manifest.json | grep -i
  content-encoding`.

## 8. Publish a new schedule

Replace the GTFS file at the `-feed` address. The server checks it
about once a minute and imports a new version on its own. Open apps
pick it up at their next check, without reloading. Nothing needs to
restart.

Planners can also publish a schedule change for a future date from the
service design app. It takes effect on its date.

## 9. Update or roll back the app

**To update,** build a new image ([section 3](#3-build-the-image)) and
replace the containers with it, one at a time. When the new version
starts, it records its pages in the bucket. Open apps download the
new version in the background and switch to it safely.

A new version can store some data in a newer format. When it starts,
it converts the data in use to that format: the edits, the schedule in
force, and the schedules still to come. Each conversion is a new entry
in that data's history, with the writer `migrate:<module>`. The old
copy stays in the bucket. An older schedule that is no longer in force
is not converted.

**To roll back,** ask the server to serve an earlier version. Every
version stays in the bucket. List them:

```
curl https://<your address>/pointers/build/log
```

Each entry's `to` is a version. Choose the one to go back to and send:

```
curl -X POST https://<your address>/builds \
  -H "Authorization: Bearer <token>" \
  -d '{"build": "sha256:…"}'
```

Open apps follow the rollback like an update. The containers do not
need to change: a server restarted on the newer image keeps serving
the version you rolled back to.

## 10. Give each app its own address

The server's start page lists every app at one address. That is fine
for testing. In production, give each app its own address, such as
`https://stops.agency.example`. Then you deploy, update, and roll back
each app without touching the others.

Each app's address is served by a web host of your choice (a load
balancer, a reverse proxy such as nginx, or a Cloudflare Worker). The
host does two things:

- it serves the app's own files;
- it sends the app's data requests on to the server.

The app and its data then share one address, so the server needs no
extra settings for it.

**1. Keep each app's versions in the bucket.** Give the server
`-ui-dist`, with each app's name and its files. The image carries every
app's files under `/var/run/ko/dist-ui/`:

```
ui-dist = "stops=/var/run/ko/dist-ui/stops,service-change=/var/run/ko/dist-ui/service-change"
```

The names are `ride-now`, `stop-display`, `stops`, `service-change`, and
`service-design`. On each start, the server records each app's files
as a version in the bucket, under the name `build.<app>`. A version
the bucket already holds is not recorded again.

**2. Write out the app's files.** From a checkout of the repository:

```
just ui-deploy https://<your server> stops out/stops
```

This writes the app's newest version into `out/stops`, and checks each
file. The folder must be empty or not exist. Upload the folder to the
app's web host.

**3. Send the data requests to the server.** Send these paths to the
server, and serve everything else from the app's files. The paths in
the first list must match exactly, because `/stops` is data and
`/stops.html` is the app's page:

- exactly: `/manifest`, `/edits`, `/feed`, `/network`,
  `/network/document`, `/network/boundary`, `/streets`, `/walks`,
  `/plans`, `/plans/retire`, `/stops`, `/stop-requests`,
  `/stop-requests/work-order`, `/attachments`, `/builds`,
  `/permissions`, `/healthz`, `/readyz`, `/basemap.pmtiles`;
- everything under: `/blobs/`, `/pointers/`, `/logs/`, `/attachments/`,
  `/geocode/`, `/basemap-assets/`.

With nginx, where `transit-server` is the server's upstream:

```
location ~ ^/(manifest|edits|feed|network|network/document|network/boundary|streets|walks|plans|plans/retire|stops|stop-requests|stop-requests/work-order|attachments|builds|permissions|healthz|readyz|basemap\.pmtiles)$ {
    proxy_pass http://transit-server;
}
location ~ ^/(blobs|pointers|logs|attachments|geocode|basemap-assets)/ {
    proxy_pass http://transit-server;
}
location / {
    root /srv/stops;   # the folder from step 2
}
```

With a Cloudflare Worker that has the app's files as its static
assets (`ASSETS`) and the server's address in `SERVER`:

```js
const exact = new Set(["/manifest", "/edits", "/feed", "/network", "/network/document", "/network/boundary", "/streets", "/walks", "/plans", "/plans/retire", "/stops", "/stop-requests", "/stop-requests/work-order", "/attachments", "/builds", "/permissions", "/healthz", "/readyz", "/basemap.pmtiles"]);
const under = ["/blobs/", "/pointers/", "/logs/", "/attachments/", "/geocode/", "/basemap-assets/"];
export default {
  fetch(request, env) {
    const url = new URL(request.url);
    if (exact.has(url.pathname) || under.some((p) => url.pathname.startsWith(p))) {
      return fetch(new Request(env.SERVER + url.pathname + url.search, request));
    }
    return env.ASSETS.fetch(request);
  },
};
```

**4. Let the app sign in at its address.** Add the app's address as a
redirect address with your sign-in provider
([section 6](#6-turn-on-sign-in), step 4).

**To update an app,** build a new image and replace the server's
containers ([section 9](#9-update-or-roll-back-the-app)). The server
records the app's new version. Then run step 2 again into an empty
folder, and upload it.

**To roll an app back,** list its versions:

```
curl https://<your server>/pointers/build.stops/log
```

Choose the version to go back to, and move the app to it:

```
curl -X POST 'https://<your server>/builds?ui=stops' \
  -H "Authorization: Bearer <token>" \
  -d '{"build": "sha256:…"}'
```

Then run step 2 again and upload the folder. The other apps do not
change. To let a role do this, give it write on `build.<app>` in the
policy (for example `build.stops`).

## 11. Look after the bucket

These tools run from a checkout of the repository, not from the
image.

- **Check the bucket** (read-only, safe at any time):

  ```
  just store verify 's3://agency-transit/prod?region=us-west-2'
  ```

  It reads everything and stops with an error at the first problem.
  It needs the modules that `just caps` builds: it uses one of them to
  check the saved summaries of detours and cancellations.

- **Clean up** files that nothing refers to any more. Use the
  credentials **with** delete permission:

  ```
  just store gc 's3://agency-transit/prod?region=us-west-2'
  ```

  It only removes files more than 7 days old, so it is safe to run
  while the servers are running. Weekly is enough.

- **Undo a wrong detour or cancellation.** Every change to detours and
  cancellations is kept, in order, with who made it. To see the latest
  changes (it needs the `audit` rule when sign-in is on):

  ```
  curl -H "Authorization: Bearer <token>" https://<your address>/logs/disruptions?since=0
  ```

  Each entry has a `seq` number, the `writer`, and the detours and
  cancellations it changed. To undo entry 42:

  ```
  curl -X POST -H "Authorization: Bearer <token>" -d '{"seq": 42}' https://<your address>/logs/disruptions/revert
  ```

  The undo is a new entry. Entry 42 stays in the history. If someone
  changed the same detour after entry 42, the server refuses (409) and
  names it, so that a person decides what it should be.

- **Undo a bad update's data** (the emergency procedure). Use this only
  when an update wrote data that the earlier version cannot use, and
  rolling the app back ([section 9](#9-update-or-roll-back-the-app))
  is not enough. First roll the app back. Then, for each kind of data
  the update changed (for example `plans`), list its history:

  ```
  curl https://<your address>/pointers/plans/log
  ```

  Choose the last entry from before the update. Its `to` is the
  version to go back to. Then run:

  ```
  just store revert -writer <your name> 's3://agency-transit/prod?region=us-west-2' plans sha256:…
  ```

  The tool moves the data back and lists every change made since then,
  with who made it. Those changes stay in the bucket. A person enters
  them again by hand in the app. Detours and cancellations are not
  undone this way: undo their entries one by one, as above.

- **Backups.** Choose where copies go and how often with your
  provider's own tools: bucket versioning and replication to a second
  site, a regular copy to a second bucket that the server's credentials
  cannot change, or both. Every file in the bucket is written once and
  never changed, so a copy is simple. Copy the `logs/` and `pointers/`
  folders first, then `blobs/`. To restore, point the server at the copy and
  run `just store verify` on it. Restore a backup only into the same
  version of the app or a newer one. An older version refuses data
  that a newer one wrote.

  Practice a restore before you rely on it, and again after each
  change to how you take copies: restore into a new, empty bucket, run
  `just store verify` on it, start a server on it, and check that the
  apps show the same schedule and detours.

## 12. Troubleshooting

| What you see | What to do |
|---|---|
| The log says `DEV STORE` | Add `-store`. Without it, nothing is saved. |
| The log says `auth OFF` | Turn on sign-in ([section 6](#6-turn-on-sign-in)). |
| `/readyz` says `degraded` | The bucket is not answering. Check the provider's status and the credentials. The apps keep working, but changes wait. |
| Saving a detour fails with 503 | The same: the bucket is not answering. The app keeps the change and tries again. |
| The server will not start, with `store:` in the error | The store address or the credentials are wrong. |
| The server will not start, with `feed:` in the error | The `-feed` address does not answer, or the file is not valid GTFS. (If the address does not answer but the bucket holds an earlier import, the server starts with that instead.) |
| The log says `ALARM disruptions log: a gap` | A file in the bucket's `logs/disruptions/entries/` folder is missing, so detours and cancellations cannot be saved. Restore it from a backup if you can. If not, record why it is missing: `curl -X POST -H "Authorization: Bearer <token>" -d '{"seq": <the missing number>, "cause": "<why>"}' https://<your address>/logs/disruptions/close-gap`. Saving works again at once. |
| The log says `ALARM disruptions log: entry … is quarantined` | One saved change cannot be read, so the server skips it and keeps going. Look at it in `/logs/disruptions` (it is listed under `quarantined`), and enter the change again by hand if it is needed. |
| A new schedule does not appear | Wait two minutes, then check that the file at the `-feed` address really changed. |

How the server works inside is in
[docs/architecture/server.md](docs/architecture/server.md), and how it
stores the data in
[docs/architecture/storage.md](docs/architecture/storage.md).
