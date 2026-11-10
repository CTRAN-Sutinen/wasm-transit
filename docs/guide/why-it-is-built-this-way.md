# Why wasm-transit is built this way

This page explains the main design choices. For each one it says what
was chosen, why, and what it costs. Every choice has a cost. Knowing
the cost tells you when the choice might be worth revisiting.

The choices are:

1. [Do the work on the rider's device](#1-do-the-work-on-the-riders-device)
2. [Run the same program everywhere](#2-run-the-same-program-everywhere)
3. [Name each document by its contents](#3-name-each-document-by-its-contents)
4. [Use a network model inside, and GTFS at the edges](#4-use-a-network-model-inside-and-gtfs-at-the-edges)
5. [Give each module one question](#5-give-each-module-one-question)
6. [Never overwrite or delete](#6-never-overwrite-or-delete)
7. [Settle shared edits with a simple rule](#7-settle-shared-edits-with-a-simple-rule)
8. [Store in object storage, not a database](#8-store-in-object-storage-not-a-database)
9. [Let the rider apps run without a server](#9-let-the-rider-apps-run-without-a-server)
10. [Read the clock in the agency's time zone](#10-read-the-clock-in-the-agencys-time-zone)
11. [Check the rules by machine, and test from intent](#11-check-the-rules-by-machine-and-test-from-intent)

## 1. Do the work on the rider's device

**The choice.** The rider's browser computes departure boards and
trip plans itself. It downloads the network once and answers every
question locally.

**Why.** A rider at a stop with no signal can still see the schedule.
A rider's trips and saved stops never leave their device. The agency
needs no computing power that grows with the number of riders,
because each phone does its own work. The agency only serves files,
and any web host does that cheaply.

**The cost.** The device does real work, so the software must be
careful with memory and speed. The project sets budgets for both, and
measures against them. The first visit downloads the data. For
C-TRAN, the network is about 0.6 MB compressed, and the street data
that walking directions use is about 4.5 MB.

## 2. Run the same program everywhere

**The choice.** Every answer comes from a WebAssembly module. The
browser, the server, and the command-line tool run the same module
files. The code around the modules (the hosts) only fetches, stores,
and draws. It never calculates an answer.

**Why.** If the browser calculated some answers itself, the server
would need a second copy of that logic, and the two would drift
apart. With one program, the browser and the server always agree,
and anyone can check an answer by asking the same question on the
command line.

**The cost.** All domain logic must be written in Rust and compiled
to WebAssembly, even small things that would be quick to write in the
web code. Adding a feature takes more steps: the logic goes into a
library, then a module, then the app.

## 3. Name each document by its contents

**The choice.** Every document is named by its content hash, a
fingerprint calculated from its exact bytes. Modules check every
document against its name before using it.

**Why.**
- A damaged or altered document is caught and refused.
- A document can be cached forever, because a name always means the
  same contents.
- Two devices, or a device and the server, can tell whether they hold
  the same data by comparing names.
- Old versions stay available by name, which makes history and
  rollback simple.

**The cost.** Checking takes time: each page computes the hash of each
document it loads. Every change makes a new document, so storage
grows over time.

## 4. Use a network model inside, and GTFS at the edges

**The choice.** The software reads a GTFS file once and turns it into
its own **network** document. Every question is answered from the
network. GTFS is used only to bring data in and to send it out.

**Why.** GTFS is designed for sharing schedules between systems. It
is not designed for planning work. Planners think in routes,
directions, patterns, and service days; GTFS stores rows in separate
files that must be kept consistent by hand. A model built for the
work makes questions simpler to answer and edits harder to get wrong.

Nothing is lost on the way. The import and export cover the whole
GTFS Schedule specification, and tests check that a feed converted to
a network and back is unchanged.

**The cost.** The translation between GTFS and the model is code to
maintain. When the GTFS specification adds something, the model must
learn it too.

## 5. Give each module one question

**The choice.** Each capability answers one question, such as "when
does service leave this stop?" It may answer that question in
several forms: for one stop, a whole station, or a saved list of
stops. It does not take on a second question. Capabilities never call
each other. They share data only through documents.

**Why.** A small module is easier to understand, test, and replace.
An app uses only the modules it needs. A module can be retired
without touching the others. The rule also stops the software from
slowly growing back into one large program where everything depends
on everything.

**The cost.** There are many modules, and a feature that touches
several questions touches several modules. Deciding whether something
is "one question" takes judgment, and that judgment happens in code
review. It cannot be checked by machine.

## 6. Never overwrite or delete

**The choice.** A change creates a new version. The old version stays.
"Deleting" something marks it as retired. The server's store only
ever adds files. The running server cannot delete anything at all; a
separate offline tool removes data that nothing refers to any more.

**Why.** Transit agencies need to answer "what did we publish, when,
and who changed it?" A store that only adds gives a complete history.
Rolling back means pointing at an older version, which is still
there. Copies on devices that have been offline stay valid.

**The cost.** Storage grows. Most documents are small, but the
document of detours and cancellations grows with every edit, and how
to keep it in check is still being decided.

## 7. Settle shared edits with a simple rule

**The choice.** Several people may edit detours and cancellations at
the same time. Each record is settled on its own: the latest change
wins, as judged by the server's clock. Retired records stay,
marked as retired.

**Why.** The agency's scheduling system is the final authority, and
changes here go through review (draft, submitted, accepted) before
they count. With review in place, a simple rule is enough. A simple
rule is also easy to explain and to check. Tests try many random
combinations of edits, arriving in different orders, and check that
every copy ends up with the same result.

More advanced approaches merge simultaneous edits automatically. They
were rejected because they are complex, and because they can hide a
conflict that a reviewer should see.

**The cost.** If two people change the same record at the same time,
one change replaces the other. Review is what catches this.

## 8. Store in object storage, not a database

**The choice.** The server keeps its data in cloud object storage:
Amazon S3 (or a compatible service), Azure Blob Storage, or Google
Cloud Storage. It relies on only two things these services all
provide: writing a whole file, and creating a file only if it does
not already exist.

**Why.** Object storage keeps data safe when a server fails. Because
the server keeps nothing important on its own disk, any server can be
replaced by a new one pointed at the same storage. There is no
database to run, tune, or back up separately. The agency can switch
providers. It can also turn on the provider's lock that prevents
deleting data for a set period, which suits audit rules.

**The cost.** The store's logic is custom code, so it is checked
heavily. A tool checks a formal model of the store, and the same
tests run against each provider's storage. When several servers
share one store, a server may see another's change up to two seconds
late. The store is not built for searching rows by their contents.
That is one reason day-to-day dispatch data stays in the systems
built for it.

## 9. Let the rider apps run without a server

**The choice.** Rider apps can be deployed as plain files on any
static web host, with no server program.

**Why.** It is the cheapest and simplest way to deploy, and the apps
keep working if a server would have failed. Each app is its own
product, so an agency can deploy, update, and roll back each one on
its own.

**The cost.** Without a server there are no staff-entered detours or
cancellations and no address search. A new schedule needs either a
new export or, if the apps follow the agency's GTFS link, a daily
check on each device. Following the link also means each device
reads the GTFS file itself, which is slower than loading a network
the server has prepared.

## 10. Read the clock in the agency's time zone

**The choice.** Every app reads the date and time in the agency's
time zone, which the deployment publishes. It never uses the device's
own time zone. Times inside the software are counted in seconds from
the start of the service day, and can run past midnight.

**Why.** A stop display set to the wrong time zone must still show the
right buses. Transit days do not end at midnight: a trip that leaves
at 00:30 often belongs to the day before. Days when the clocks change
have 23 or 25 hours, and the software must count them correctly.

**The cost.** Every app must learn the agency's time zone before it
can show anything. Tests run from a computer set to a different time
zone to catch mistakes.

## 11. Check the rules by machine, and test from intent

**The choice.** The rules above are not just written down. Automated
checks enforce them on every change. For example:

- a module that depends on another module fails the build;
- a module that tries to store its own data fails the build;
- every example in the module documentation is run, and must give
  the same answer from a module on its own and from inside each app;
- the web code is checked so that it never calculates answers.

Tests are written from what the behavior is meant to be (the agency's
intent and the GTFS specification), before the code. A test's
expected answer is worked out by separate code, so a mistake in the
software cannot also be a mistake in its test. Every module is also
tried on a real agency's data (C-TRAN's) before it counts as done.

**Why.** Rules that are only written down drift. Tests written by
looking at the code tend to confirm what the code does, including
its bugs. This project has shipped exactly that kind of bug before.

**The cost.** The full set of checks takes about 15 minutes to run.
Adding anything new means writing its tests first, and a new module
needs tests at several levels before it is done.

## Questions still open

Some choices are not settled yet. Each open question, with its
options, is in [DECISIONS.md](../DECISIONS.md).

## Read more

Each architecture file has a "Why" section with the full reasoning
behind its choices, including the alternatives that were rejected:

| Choices | Where |
|---|---|
| 2, 3, 5 | [architecture/bus.md](../architecture/bus.md) |
| 4 | [architecture/crates.md](../architecture/crates.md) and [crates/AGENTS.md](../../crates/AGENTS.md) |
| 6, 7, 8 | [architecture/server.md](../architecture/server.md) and [FORMATS.md](../FORMATS.md) |
| 9 | [architecture/static-deployment.md](../architecture/static-deployment.md) |
| 1, 10 | [architecture/updates-offline.md](../architecture/updates-offline.md) and [architecture/web.md](../architecture/web.md) |
| 11 | [TESTING.md](../TESTING.md) |
