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
10. [Look after the bucket](#10-look-after-the-bucket)
11. [Troubleshooting](#11-troubleshooting)

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
   the clean-up tool only ([section 10](#10-look-after-the-bucket)).

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

**Check it.** Open `https://<your address>/readyz`. It should say
`{"status":"ready","store":"ok"}`. Then open the address itself: the
start page lists the apps.

**If the log says `DEV STORE`,** the server is running without a
bucket. Edits are kept in memory and lost when it stops. Add `-store`.

## 6. Turn on sign-in

Without sign-in, anyone who can reach the server can enter detours.
The log says `auth OFF` at startup. Turn sign-in on before staff use
it.

1. Register the server with your sign-in provider. Note the
   **issuer**, the **audience** (client ID), and the address of its
   **signing keys** (JWKS). Set up the provider to put each user's
   roles in the sign-in token, in a claim named `roles`.
2. Write a policy file that says which role may change what. For
   example:

   ```json
   {"kind": "policy", "schema_version": 1, "rules": [
     {"name": "dispatch-edits", "roles": ["dispatcher"], "actions": ["write"], "pointers": ["edits"]},
     {"name": "planning",       "roles": ["planner"],    "actions": ["write"], "pointers": ["plans"]},
     {"name": "releases",       "roles": ["admin"],      "actions": ["write"], "pointers": ["build"]}
   ]}
   ```

   `edits` is detours and cancellations, `plans` is published schedule
   changes, and `build` is the app version
   ([section 9](#9-update-or-roll-back-the-app)). Everyone can read.
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

The log of every change names the person who made it.

**Not yet:** the app pages do not sign in yet. With sign-in turned
on, changes made in the apps (such as publishing from service design)
are refused. Tools that send their own sign-in token still work.

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

## 10. Look after the bucket

These tools run from a checkout of the repository, not from the
image.

- **Check the bucket** (read-only, safe at any time):

  ```
  just store verify 's3://agency-transit/prod?region=us-west-2'
  ```

  It reads everything and stops with an error at the first problem.

- **Clean up** files that nothing refers to any more. Use the
  credentials **with** delete permission:

  ```
  just store gc 's3://agency-transit/prod?region=us-west-2'
  ```

  It only removes files more than 7 days old, so it is safe to run
  while the servers are running. Weekly is enough.

- **Backups.** Every file in the bucket is written once and never
  changed, so a copy is simple. Copy the `pointers/` folder first,
  then `blobs/`. To restore, point the server at the copy and run
  `just store verify` on it. Where backups go and how often is still
  being decided.

## 11. Troubleshooting

| What you see | What to do |
|---|---|
| The log says `DEV STORE` | Add `-store`. Without it, nothing is saved. |
| The log says `auth OFF` | Turn on sign-in ([section 6](#6-turn-on-sign-in)). |
| `/readyz` says `degraded` | The bucket is not answering. Check the provider's status and the credentials. The apps keep working, but changes wait. |
| Saving a detour fails with 503 | The same: the bucket is not answering. The app keeps the change and tries again. |
| The server will not start, with `store:` in the error | The store address or the credentials are wrong. |
| The server will not start, with `feed:` in the error | The `-feed` address does not answer, or the file is not valid GTFS. (If the address does not answer but the bucket holds an earlier import, the server starts with that instead.) |
| A new schedule does not appear | Wait two minutes, then check that the file at the `-feed` address really changed. |

How the server works inside is in
[docs/architecture/server.md](docs/architecture/server.md).
