# transit-alerts

The service alerts document and the alert templates document (D-206..D-210). An alert is what the agency tells riders. Dispatch and the communications staff write alerts, and every team reads the one list. cap-alerts is the seam, and transit-rt writes the feed.

**An alert** is a record by id. The id is its GTFS-rt entity id. It has:
- its status: draft, published, or retired (withdrawn). There is no review step.
- its cause, effect, and severity (the enums of this crate, with the alert period; transit-live uses them);
- its header, description, and link, each a list of texts with their languages;
- its communication periods and impact periods, in Unix seconds. The host converts the agency's local times (D-205).
- its informed entities, in the network's ids: a route, a route type, a direction of a route, a stop, or a trip instance. Every present field must match. An entity with every field absent is the whole agency.
- the disruptions it explains: an edit record, a stop's suspension, or a trip instance that the live feed cancels (version 2, D-216). The chain from version 1 lives here (`version`): every v1 document is a v2 document. `version::record_forward` brings a stored log record forward.

**A template** is a record by id: its name in the menu, the cause, effect, and severity it sets, and its texts with blanks (`{stop}`, `{route}`, `{start date}`, `{end date}`). A template no longer offered is `retired`.

**Operations.**
- `checked` refuses only what makes a record wrong. Each refusal names its alert or template.
- `flags` tells a person about an alert that passes: an id the network does not have, or a missing translation for a language the deployment names.
- `put` places one record by id. `end` ends an alert at a moment, and changes nothing before it.
- `list` gives the agency's list at a moment: in force, upcoming, ended, and drafts. An alert in force with no end says how long it has been open.
- `offered` gives the templates a menu offers, by name. A retired template is kept and not offered.
- `fill` writes a template's blanks: names from the network, and dates and lists in English. The input may give the alert's informed entities, and each route and stop in them is named once. A blank it cannot fill stays, and is named.

Each document is canonical as it stands (records by id). The crate keeps what it does not know.

## Why

Each entry covers one choice that other code depends on. The heading
names the code it governs. **Holds** is what must stay true. Change it
only with the user, after you read **Because**.

### A service alert is its own record, in the network's ids (transit-alerts, cap-alerts, rt `encode_alerts`, `read_alerts`)

- **Holds:** an alert is a record of its own, in a document of its own, by id. It is not a payload of the edit document: the board and the router patch the day from edits, and an alert changes no service.
- **Holds:** an alert names what it is about in the network's ids. GTFS-rt is its serialization: transit-rt writes it as an Alerts feed and reads a feed into it. Each direction gives back the other.
- **Holds:** an alert keeps every translation of its texts. The live document carries one (D-20).
- **Holds:** there is no review step. A person with the write role publishes at once, and the log records who wrote what.
- **Holds:** an alert explains a disruption by naming it: an edit record, a stop's suspension, or a trip instance that the live feed cancels, in one list (`explains`). Nothing writes an alert from a detour, a cancel, or a closed stop. The gap list shows what no alert explains.
- **Because (the trip instance):** on C-TRAN the live feed is where cancels come from (D-213). A separate field for feed cancels would give one idea two fields forever, and matching an alert's informed entities to a cancel is a rule, not a recorded fact. The document was new, so its version changed when it cost least (D-216).
- **Holds:** a template's blanks are filled by an op, so a stop's name and a date read the same in every alert.
- **Because:** dispatch knows what is happening on the road now, and the communications staff write the agency's standard language. Both write in one place, and every team reads the one list. The record replaces a custom app whose feed riders' apps read.
- **Rejected:** a third payload in the edit document; an alert written from a detour automatically; a required review.
- **Decided:** D-206, D-207, D-208, D-210, D-216.
