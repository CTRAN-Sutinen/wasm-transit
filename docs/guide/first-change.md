# Making your first change

This page is for a developer who is new to the project. It follows one
real change from start to finish: a new question that the Service
change app needed answered. You will see every file the change touched,
in order, and which steps need your judgment.

Read [How it works](how-it-works.md) first. It explains the four
building blocks this page uses: documents, capabilities, hosts, and the
kernel. The [glossary](glossary.md) defines every term.

## Before you start

1. Set up the toolchains and install the web packages, as the
   [README](../../README.md) describes.
2. Build the modules with `just caps-fast`. This is the fast build for
   day-to-day work. It takes about 10 seconds after a library change.
3. Run the checks with `just test`. They should pass before you change
   anything.

## The example

A planner builds a new route in the Service change app. The route needs
stops that the Stops app created, but the planner's draft does not have
them yet. So the app has to ask: "What is the record of each of these
stops on this date, in the network's terms?"

That question is about the stops document, so it belongs to the
**stops** capability. The change added one **op** to that capability,
called `records`, and used it in the app.

## The steps at a glance

| Step | File | Who does the work |
|---|---|---|
| 1. Write the laws | `crates/transit-stops/tests/plan_takes_stops_law_test.rs` | You decide what is correct |
| 2. Write the library function | `crates/transit-stops/src/stop_record.rs` | You write the logic |
| 3. Add the op to the capability | `crates/cap-stops/src/lib.rs` | Routine: about 10 lines |
| 4. Describe the op | `crates/cap-stops/OPS.md` | Routine; the build checks it is there |
| 5. Add a worked example | `crates/cap-stops/src/examples.rs` | Routine; the build runs it |
| 6. Test the op | `crates/cap-stops/tests/dispatch_test.rs` | Routine |
| 7. Add the op to the app's list | `web/src/uis/service-change/ui.json` | One line |
| 8. Rebuild | `just caps-fast` | The build generates the rest |
| 9. Call the op from the app | `web/src/uis/service-change/host.svelte.ts` | You decide when to ask |
| 10. Show the answer | a component in `web/src/components/` | You design the screen |
| 11. Test in the browser | `web/e2e/ui-service-change.spec.mjs` | You write the test |
| 12. Run all the checks | `just check` | The build |

Four steps need real thought: 1, 2, 9, and 10. Any application needs
that thought. The other steps follow a pattern, and the build tells you
when one is missing.

## 1. Write the laws

Start with the tests that say what is correct, before any code. In this
project those tests are called **laws**. Base them on what the behavior
should be, never on what the code does. The law test opens with the
rules it checks, in words:

```rust
//! * `records(stops, ids, on)` answers each named stop's record in the
//!   network's terms on `on`, exactly the record a published plan reads;
//! * a stop that is not there yet on `on` is not answered; a stop the
//!   document does not hold is not answered;
//! * the record is copied, never linked: the stops document is not
//!   written.
```

Run the laws before you write the function. They should fail.
[TESTING.md](../TESTING.md), "Laws first", explains the rules.

## 2. Write the library function

All the transit logic lives in libraries: the crates named
`transit-*`. A library function takes everything it needs as
arguments. It never reads the clock, so the date comes in as `on`.

```rust
pub fn records(stops: &Stops, ids: &[String], on: NaiveDate) -> BTreeMap<String, Stop> {
    ids.iter()
        .filter_map(|id| stops.stop_on(id, on).map(|found| (id.clone(), record(&found))))
        .collect()
}
```

This is the only new logic in the whole change. Run the laws again. Now
they should pass.

## 3. Add the op to the capability

A capability is a thin wrapper around a library. It adds one request,
one response, and one line of dispatch for each op:

```rust
// The request: what the caller sends.
Records { stops_hash: String, stop_ids: Vec<String>, on: String },

// The response: what it gets back.
Records { records: BTreeMap<String, transit_network::Stop> },

// The dispatch: read the request, call the library, answer.
Request::Records { stops_hash, stop_ids, on } => {
    let on = parse_date(&on)?;
    let stops = STOPS.require(&stops_hash)?;
    Ok(Response::Records { records: transit_stops::stop_record::records(&stops, &stop_ids, on) })
}
```

`kind::<Stops>().require(&stops_hash)` asks the kernel for the stops
document by its content hash. A capability never keeps documents of its
own.

## 4. Describe the op

Each capability has an `OPS.md` file, with one section for each op. The
section says what the op answers. Agents and developers read it, and
the manifest publishes it. The build fails if an op has no section.

## 5. Add a worked example

Each op has one worked example: a request and its answer. You write the
request, and the documents it needs, in the capability's
`src/examples.rs`, beside the others. The build computes the answer by
running the op, so the example cannot go out of date. You do not list
the op anywhere else: the build reads the ops from the capability's
`Request` enum.

The build also runs every example inside each app's module. If the
answer differs there, the build fails.

## 6. Test the op

The dispatch test sends the op JSON and checks the JSON it gets back.
It is the same request an app sends.

```rust
let taken = ok(json!({"op": "records", "stops_hash": planned["stops_hash"],
                      "stop_ids": ["N1", "A", "Z"], "on": "2026-10-20"}));
assert_eq!(taken["records"]["N1"]["name"], "Washougal TC");
assert!(taken["records"].get("Z").is_none());
```

## 7. Add the op to the app's list

Each app lists the ops it uses in its `ui.json` file. Add one line:

```json
"stops.records",
```

The build uses this list to decide which capabilities go into the app's
module.

## 8. Rebuild

Run `just caps-fast`. It generates the app's module, the manifest with
your worked example, and the TypeScript types. You do not edit any of
these by hand.

## 9. Call the op from the app

Each app has one host file. The host asks questions through the
**engine**, with `ask(capability, op, arguments)`. The types from step
8 check the arguments and the answer.

```ts
const records = (await ask("stops", "records", {
  stops_hash: record, stop_ids: taken, on: periodFrom(),
})).records;
```

The host then adds each record to the planner's draft as an ordinary
stop. Here you decide when to ask, and what to do when a stop is
missing.

The host never works out an answer itself. If the screen needs a
number, a date, or a decision, an op answers it. This rule is why the
browser, the server, and the command line always agree.

## 10. Show the answer

Components only draw what they are given. They do not call ops. Here
the new-route form lists the stops the planner picked.

## 11. Test in the browser

Add or extend the app's browser test in `web/e2e/`. The test for this
change builds a new route from start to finish. Run one test while you
work: `just e2e e2e/ui-service-change.spec.mjs`. The path is from
the `web/` folder.

## 12. Run all the checks

- While you work: `just test`, and `just e2e` with one test.
- Before you finish: `just check`. It runs every check, including the
  browser tests.
- When a whole piece of work is done: `just check-release`. It runs the
  same checks on the release build, which takes about an hour to
  build.
- Try the change on real data. `just qa` starts the apps on C-TRAN's
  schedule.

Then update the documents that describe what you changed.
[CONVENTIONS.md](../CONVENTIONS.md), "The documentation sweep", lists
them.

## Parts you used but did not change

- **The engine** (`web/src/engine.ts`). You called it in step 9 with
  `ask()`. It sends the request to the module and returns the answer.
  You do not change it to add an op.
- **The kernel** (`crates/cap-abi/src/kernel.rs`). It held the stops
  document for your op. `kind::<Stops>().require` in step 3 asked it
  for the document. You do not change the kernel, even to add a new
  kind of document. A new kind gets one line in the list of kinds
  (`crates/cap-abi/src/kinds.rs`), and its library says how the kernel
  loads it.
- **The generator** (`crates/generator`). It builds the manifest from
  every capability's declaration and worked examples. It names no
  capability, so you do not change it to add an op or a capability.

## Files the build wrote for you

`just caps-fast` in step 8 wrote these files. Do not edit them:

- the app's module, in `assemblies/`;
- the manifest, with your worked example;
- the TypeScript types that `ask()` checks.

## Parts a new op does not need

You can make many changes before you learn these parts. Each one has
its home in the [architecture reference](../architecture/README.md).

- **The module interface.** The four functions every module gives a
  host. A macro writes them.
- **The bus worker.** The background thread that runs a page's module.
- **The server.** The program that saves shared changes and publishes
  the schedule.
- **The store.** Where the server keeps the agency's data, in cloud
  storage.

## When the build fails

The build checks the project's rules, and its message names the rule.
The common ones on a first change are these:

- **An op has no section in `OPS.md`, or the sections are in a
  different order from the ops.** Fix the file.
- **A worked example fails, or gives a different answer inside an
  app's module.** Fix the example or the op.
- **A capability uses another capability.** Capabilities never call
  each other. Move the shared logic into a library.
- **Code in `web/` calls a module directly.** Only the engine does.
  Ask through `ask()`.

[crates/AGENTS.md](../../crates/AGENTS.md) lists every rule and the
check that enforces it.

## Read more

| Topic | Where |
|---|---|
| Where a new op belongs | [crates/AGENTS.md](../../crates/AGENTS.md), "Where new things go" |
| How tests are written | [TESTING.md](../TESTING.md) |
| How an app is built | [architecture/generated-uis.md](../architecture/generated-uis.md) |
| The engine and the browser rules | [architecture/web.md](../architecture/web.md) |
| How to write comments and documents | [CONVENTIONS.md](../CONVENTIONS.md), "Writing" |
