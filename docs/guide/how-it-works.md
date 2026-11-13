# How wasm-transit works

This page explains the four building blocks of wasm-transit, then
follows one stop display from the moment it starts to the moment its
departures appear. The last sections cover what happens when the
schedule changes and when staff enter a detour.

## The four building blocks

Every part of the software is made of four kinds of thing.

```
   documents          modules              hosts                 the core
   ---------          -------              -----                 --------
   the data           the questions        the programs that     the part of every
   (schedule,         ("when does the      run the modules       module that holds
   live data,         bus leave this       (browser, server,     documents and
   detours)           stop?")              command line)         checks them
```

### Documents: the data

A **document** is one piece of data in a file: the agency's network,
a snapshot of the live feed, the list of detours, a rider's saved
stops. Documents are plain JSON or a standard format such as GTFS.

Every document has a **content hash**: a long code calculated from its
exact contents, like a fingerprint. Two documents with the same
contents always have the same hash. If even one character changes,
the hash changes. The software uses the hash as the document's name.
This has two effects:

- A document can be checked: compute its hash and compare it with the
  name it was given. If they differ, the document was damaged or
  tampered with, and the software refuses it.
- A document never changes. A changed document is a new document with
  a new name. The old one is still there.

The most important document is the **network**: the agency's routes,
stops, trips, and service days, in the software's own model. The
software reads a GTFS file once and turns it into a network document.
Everything else works from the network, not from the GTFS file.

### Modules: the questions

A **module** is a small program that answers one kind of question
about documents. Each one is called a **capability**. For example:

| Capability | The question it answers |
|---|---|
| board | When does service leave this stop? |
| router | How do I get from here to there? |
| directory | What exists in the network (stops, routes)? |
| rt | What does the live feed say, in the network's terms? |
| analysis | How does the network perform (frequency, coverage, cost)? |

You ask a capability a question by sending it a request, such as
"the next departures at stop 106 on 10 November, from 08:00". The
request names its documents by hash. The answer is plain JSON.

Modules are compiled to **WebAssembly**, a standard format for
programs that browsers and servers can both run safely. The same
module file runs in a browser, on the server, and on the command
line, and gives the same answer in each.

An app usually needs several capabilities. When the app is built,
the capabilities it uses are joined into one module for that app.
That way, each document is loaded once per page, not once per
capability.

### Hosts: the programs around the modules

A **host** is the program that runs modules. It does everything a
module cannot do on its own: fetch files, read the clock, save data,
draw the screen. There are three hosts:

- **the browser**, which runs the apps;
- **the server**, a small Go program;
- **caps-run**, a command-line tool for developers and AI agents.

Hosts do not answer questions themselves. If the screen shows a
departure time, a module calculated it. This rule is what makes the
three hosts agree.

### The core: holding and checking documents

Every module contains the same small **core** (the code calls it the
kernel). The core loads documents, checks each one against its hash,
holds them in memory, and drops them when the host says they are no
longer needed. A capability never stores data of its own. It asks the
core for the documents it needs.

## Following one stop display

A stop display is a screen at a stop, often a small computer such as
a Raspberry Pi in kiosk mode. Its web address names the stop, for
example `stop-display.html?stop=106`. Here is what happens after it
starts.

### 1. The page loads and prepares to work offline

The browser loads the page. A **service worker**, a small script the
browser keeps, saves the page and its modules on the device. The next
time the display starts, it can start even with no network
connection.

### 2. The display learns the agency's time zone

The display reads the deployment's **manifest**: a small file that
says which documents are current, by hash. The manifest also names
the agency's time zone. From now on, the display reads the clock in
the agency's time zone, not the device's. A screen set to the wrong
zone still shows the right departures.

### 3. The display finds today's schedule

An agency may publish schedule changes ahead of time. Each one is a
**pick**: a version of the network that takes effect on a date. If
the manifest lists published picks, the display asks the module which
pick is in force today. Otherwise it uses the network the manifest
names.

### 4. The display loads the network and checks it

The display downloads the network by its hash from
`blobs/<hash>.gz`, still compressed. It passes the compressed bytes
straight to the module. The core unpacks them, reads them, and
computes their hash. If the hash does not match the name, the core
refuses the document. A display never shows a schedule that differs
from the one the agency published.

The modules run in a **background worker**, separate from the code
that draws the screen. Heavy work never makes the screen freeze.

### 5. Every 20 seconds, the display polls

While the screen is visible, the display repeats these steps every 20
seconds:

1. It fetches the agency's live feeds (trip updates, vehicle
   positions, and alerts) directly from the agency.
2. The **rt** capability decodes each feed and joins it to the
   network. The result is the **live document**: delays, alerts, and
   cancellations, in the network's terms.
3. If the deployment has a server, the display also fetches the
   detours and cancellations staff have entered.
4. The display checks the manifest again. If the agency has
   published a new schedule, the display switches to it without
   reloading the page.

### 6. The board capability answers

The display asks the **board** capability one question, with
everything it needs:

- the network, the detours, and the live document, by hash;
- the stop;
- today's date and the time now, from the agency's clock;
- the previous day's date, so a trip that started before midnight and
  is still running appears too.

The board builds the day's trips from the network, applies the
detours, and reads the live delays. It answers with one group for
each route and destination at the stop. Each group gives:

- its next three departures;
- how long until each one leaves;
- whether each time is live or from the schedule;
- the alerts and detours that affect it.

### 7. The display draws the answer as given

The display draws the groups exactly as the board answered. It does
not sort, filter, or calculate anything. If the live feed is down,
the board says the times are scheduled, and the display shows that.

### 8. The display tidies up

After each change, the display tells the core which documents it
still needs. The core drops the rest. This keeps memory flat, so a
display can run for weeks without restarting.

## When the schedule changes

The agency publishes a new GTFS file. With a server, the server reads
it once, turns it into a network document, and puts the new hash in
the manifest. Without a server, the agency exports a new folder, or
the apps fetch the GTFS file themselves and notice the change at their
daily check.

Each open page notices the new manifest at its next poll and switches
to the new network. Pages never stop to reload. The old network is
still available by its hash, so a page that has not switched yet keeps
working. In a deployment without a server, rolling back means
uploading the old manifest again.

A new version of the app itself works the same way. The browser
downloads it in the background and waits. A stop display switches at
a poll, and only while the deployment is reachable. A rider's phone
switches the next time the app opens, or when the rider accepts the
update. Nothing changes under a rider in the middle of a task.

## When staff enter a detour

This needs the server. A detour or a trip cancellation is a
**record** in the **edit document**. A record moves through a simple
workflow: draft, submitted, accepted, and finally retired when it no
longer applies.

Several people may edit at once. The rule for conflicts is simple:
for each record, the latest change wins, and "latest" is judged by
the server's clock. A retired record stays in the document, marked
retired, so the history is complete.

The server keeps each version of the edit document in its store, and
logs every save. When sign-in is turned on, the log names the person
or system that made each save. The rider apps fetch the
current edit document when they poll, and the board applies its
detours and cancellations to the day.

## Try it yourself

You can ask the same questions the apps ask, from the command line.
First build the modules (see the [README](../../README.md)):

```
just caps
```

List the capabilities:

```
node tools/caps-run.mjs list
```

See what the board capability can answer, with an example for each
question:

```
node tools/caps-run.mjs tools board
```

Ask for the departures at a station in the small test network. The
first `--op` loads the network. The second asks the board, naming the
network by the hash the first step returned:

```
node tools/caps-run.mjs board \
  --op @web/public/caps/sample-network-load.json \
  --op '{"op":"stop_board","network_hash":"$load_network.network_hash","stop_id":"STN","date":"2026-10-23","from_secs":25200,"limit":10}'
```

The answer lists each departure with its route, its bay, where it is
going, and its time. Times are seconds since the start of the service
day: 25200 is 07:00, and 29700 is 08:15. `"pooled": true` means the
station has no departures of its own, so it shows its bays' departures
together. `"status": "no_prediction"` means no live data was given, so
the time is the schedule's.

This is the same module, giving the same answer, as the browser runs.

## Read more

| Topic | Where |
|---|---|
| Modules, documents, and the core in detail | [architecture/bus.md](../architecture/bus.md) |
| The browser side: the engine and the background worker | [architecture/web.md](../architecture/web.md) |
| Updates and offline use | [architecture/updates-offline.md](../architecture/updates-offline.md) |
| The server | [architecture/server.md](../architecture/server.md) |
| How the data is stored | [architecture/storage.md](../architecture/storage.md) |
| Deploying with no server | [architecture/static-deployment.md](../architecture/static-deployment.md) |

## Read next

[Why it is built this way](why-it-is-built-this-way.md) explains the
choices behind these steps.
