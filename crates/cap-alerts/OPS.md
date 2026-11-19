# alerts

What the agency tells riders (library: transit-alerts). The service alerts document holds the agency's alerts in the network's ids. Dispatch and the communications staff write alerts, and every team reads the one list.

An alert has these parts:

- its status: `draft`, `published`, or `retired` (withdrawn). There is no review step. A person with the write role publishes at once.
- its cause, effect, and severity, by GTFS-rt's names;
- its header, description, and link, each a list of texts with their languages;
- when to show it (`communication`) and when service is affected (`impact`), in Unix seconds. The host converts the agency's local times. No communication period means for as long as it is published.
- what it is about (`informed`): each entity names a route, a route type, a direction of a route, a stop, or a trip instance, in the network's ids. Every present field must match. An entity with every field absent is the whole agency.
- the disruptions it tells riders about (`explains`): an edit record by id, a stop's suspension by the stop's id, or a trip instance that the live feed cancels (`trip`: the trip, its service date, and its start; version 2).

The service alerts document is version 2. cap-migrate brings a version 1 document forward, and every version 1 alert is a version 2 alert. The alerts log keeps each record in the version it was written in, and `log_document` brings each one forward.

Nothing is deleted. A withdrawn alert is `retired`, and an alert that ends gets its end (`end`).

The templates document holds the agency's standard language. A template sets the cause, effect, and severity, and its texts hold blanks: `{stop}`, `{route}`, `{start date}`, and `{end date}`.

A service alerts document registers on the kernel's `load_service_alerts`, and a templates document on `load_alert_templates`. Each load is refused unless `validate` or `validate_templates` would pass the document. cap-rt's `encode_alerts` writes the document as a GTFS-rt Alerts feed.

## validate

What a write must pass. The op lists every refusal, and each refusal names its alert and its problem.

The refusals are:

- an empty id (`no_id`);
- no informed entity (`no_entity`);
- no header text that is not blank (`no_header`);
- a period that ends before it starts (`end_before_start`);
- an impact period outside every communication period (`impact_outside_communication`), the GTFS-rt spec's rule;
- a direction with no route (`direction_without_route`);
- a trip date or start with no trip (`instance_without_trip`);
- a date that is not YYYY-MM-DD (`bad_date`);
- two texts of one field in the same language (`repeated_language`).

An id that the network does not have is not refused. `flags` tells a person about it.

With no refusal, the op answers the document's canonical bytes and their hash. The kernel's `load_service_alerts` answers the same hash. A field that this build does not know is kept, and named in `unknown_fields`.

## put

The document with one alert placed under its id, new or replacing the one held, then checked as a write is. The answer is every refusal, or the next document registered under its hash. With no `alerts_hash`, the first alert begins an empty document.

## end

The document with one alert ended at `at`, the host's clock in Unix seconds:

- A period that holds at `at` or later ends at `at`.
- A period that starts after `at` goes.
- An alert with no period gets one that ends at `at`.
- An alert with no period left at or before `at` was never told to riders, so it is withdrawn (`retired`).

Nothing up to `at` changes. An id that the document does not hold is an error.

## list

The agency's list at `now`, the host's clock in Unix seconds. Every alert is in one group, in id order:

- `in_force`: published, and a communication period holds now.
- `upcoming`: published, and a communication period starts later.
- `ended`: published and ended, or withdrawn.
- `drafts`.

An alert in force with no end carries `open_secs`: how long it has been told to riders. So a long-open alert is visible. With no `alerts_hash`, the list is empty.

## flags

What a person is told about the alerts that are not withdrawn. Nothing here refuses a write:

- an informed route, direction, stop, or trip that the network does not have (`unknown_route`, `unknown_direction`, `unknown_stop`, `unknown_trip`);
- a text with no translation for a language of `languages`, the deployment's languages (`missing_translation`, naming the field and the language).

## validate_templates

What a template write must pass. The op lists every refusal, and each refusal names its template and its problem:

- an empty id (`no_id`);
- no name (`no_name`);
- no header text that is not blank (`no_header`);
- a blank that is not one of `{stop}`, `{route}`, `{start date}`, and `{end date}` (`unknown_blank`). It could never be filled.
- two texts of one field in the same language (`repeated_language`).

With no refusal, the op answers the document's canonical bytes and their hash. The kernel's `load_alert_templates` answers the same hash.

## put_template

The templates document with one template placed under its id, then checked as a write is. The answer is every refusal, or the next document registered under its hash. A template that is no longer offered is `retired: true`, and is kept.

## fill

One template written out for a set of stops, routes, and dates:

- `{stop}` and `{route}` are the names the network gives them: "Main & 1st", or "10, 20, and 40".
- `{start date}` and `{end date}` are written from "YYYY-MM-DD" as "Monday, August 17".

The fill writes dates and lists of two or more only in English (a text with no language, or `en`). A blank that it cannot fill stays as written, and `unfilled` names it with its field and language, for a person to fill by hand. A blank cannot be filled when nothing is given for it, an id is not in the network, or a date is not YYYY-MM-DD. The answer also carries the template's cause, effect, and severity.

## log_records

The documents as the alerts log's records: a `service_alert` record for each alert, an `alert_template` record for each template, and each document's own record, which keeps the fields outside its collection. Each document left out gives no records. The log capability's `compose` takes the answer.

## log_document

One document assembled from the alerts log's records of its kind (`kind`: `service_alerts` or `alert_templates`), registered and answered as its canonical bytes and hash (`document_hash`). A record in another version is refused, naming both versions.
