# The hosts

A host runs the capability modules and does the work a module cannot
do: I/O, storage, and the clock. There are three hosts:

- the server (`server/`, Go and wazero; [server.md](server.md));
- the browser: the engine, the kit, the outbox, the service worker,
  and each UI's host layer ([web.md](web.md),
  [updates-offline.md](updates-offline.md));
- caps-run, the agent runner (`tools/caps-run.mjs`;
  [registry.md](registry.md)).

The hosts share only the modules. So the rules that every host keeps
are written here, once. Each host's own file holds its detail.

## The parts of a host

Each host has four parts:

- **An upstream.** The upstream is where the host reads documents
  from, and where it sends writes.
- **A working copy.** The working copy is what the host keeps of what
  it read from its upstream.
- **The modules.** The modules compute every answer.
- **A clock.** The host reads the clock in the agency's time zone.

| | Server | Browser | caps-run |
|---|---|---|---|
| Upstream | the bucket | the server, or the export's files | the files and URLs its user names |
| Working copy | heads and blobs in memory; with `-store-cache`, the same on disk | IndexedDB (documents, plans, the outbox, the last copy of each read) and the service worker's caches (the pages, the modules, the last copy of a rider page's reads) | the session folder (`.caps-sessions/NAME/`) |
| Modules | single-capability modules in wazero | the page's linked assembly in its bus worker | any module, or a page's assembly for a replay |
| Clock | the server's clock | the device's clock, in the agency's zone (`web/src/clock.ts`) | the machine's clock, in the zone of `--tz` or the session's network |

## The chain

The hosts form a chain. Each host sees only the host or the store above
it.

- With the server: the bucket, then the server, then the page.
- With no server: the export's files, or the agency's GTFS URL, then
  the page ([static-deployment.md](static-deployment.md)).

Each upstream answers two kinds of read:

- **A document by hash** (`blobs/<hash>`). Its bytes never change.
- **A head by name** (the `manifest`, a pointer, a log's head). A head
  names the hash of the current document.

The server answers the same read paths as an export. So one version of
the page code reads from either upstream. Writes go only to the
server, on its own paths (`POST /edits`, `POST /logs/<log>`,
`POST /plans`).

## The rules every host keeps

1. **A copy by hash is never old.** Only a head can be old. A host
   keeps a document by hash for as long as it likes, and checks only
   its heads again.
2. **The upstream wins.** The first time the upstream answers, its
   heads replace the working copy's heads.
3. **An old answer says its age.** A host that answers from its
   working copy labels the answer with the time its upstream last
   confirmed it:
   - the server: `X-Store-Stale` and `X-Store-As-Of`;
   - the service worker: `X-Served-From: last-copy`;
   - the kit: `staleLabel` and `lastGoodText`.
4. **The age passes down the chain.** A page that reads a stale answer
   from the server shows the server's as-of time. It does not show the
   time it read the answer.
5. **A working copy is never the record.** A host writes nothing from
   its working copy back to its upstream. Losing a working copy loses
   nothing of the record.

## Where the hosts differ

The hosts differ in three ways. Each difference is a design decision.

- **Authority.** The bucket is the record. A server copy and a
  device's copy are working copies. The receive path runs only on the
  server, which checks every write before the bucket takes it
  ([agency-data.md](agency-data.md)).
- **Writes while the upstream does not answer.** The server refuses
  each write with `503` and `Retry-After`. A staff page keeps each
  write in its outbox on the device, and sends it when the server
  answers. A rider page writes only its own documents on the device.
- **The clock.** The server's clock stamps the record. The time a
  device gives is a claim, kept beside the stamp (D-222).

## Where a host does not keep these rules

- The server stops a bucket call at its time limit (20 s; 2 min for a
  create) and counts it as no answer. A read in the browser has no
  time limit. A read that never answers holds the page's poll, and the
  page keeps its last answers with no label.
- The engine's served reads (`servedStops`, `servedNetwork`, and the
  others) answer null both when the server keeps no document and when
  the read failed (QAF-131). A failed read of the edit document shows
  no label (QAF-130).
- The rules are written here, and implemented twice: in Go and in
  TypeScript. No suite runs them against each host.

The open work is [ui-host-inputs.md](../units/ui-host-inputs.md).

## Why

### One set of rules for every host (this file)
- **Holds:** the server, the browser, and caps-run keep the rules
  above. Their differences are the three listed, and no others.
- **Because:** the hosts share only the modules. A rule that each
  host's file states for itself drifts. A reader who learns the rules
  once can read any host. A difference that is not named looks like a
  defect, or hides one.
- **Rejected:** shared host code. The server is Go and the browser is
  TypeScript, and the modules are the only code that every host runs.
