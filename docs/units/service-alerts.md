# Service alerts: the agency's record, and the real-data check

The service alerts document, the alert templates document, cap-alerts,
and cap-rt's `encode_alerts` are on the bus (transit-alerts, cap-alerts,
and transit-rt in `docs/architecture/crates.md`; D-206..D-211). What
is left is keeping the two documents in the agency's record, and
proving the encode against the feed that riders' apps read today. The
work replaces the custom app that C-TRAN's communications staff use
(D-207).

The page is the disruptions UI ([ui-disruptions.md](ui-disruptions.md),
D-209). Serving the feed is not in this unit, and it may be outside
this repo. The bucket is open to read, so the alerts document's layout
in the bucket is the contract that a serving endpoint reads.

## Scope, in order

1. **The log.** cap-alerts gets `log_records` and `log_document` for
   both documents, as cap-requests has (D-197). Each kind gets its row
   in the kind table (`crates/cap-abi/src/bin/common/kind_table.rs`,
   D-202) and a log under `logs/` through cap-log.
2. **The server.** The two documents are stored with their log, and
   they are written through the receive path under sign-in: `validate`
   and `validate_templates` check a write, and `flags` is what the
   answer tells a person. The server gets the cap-alerts module
   (`just caps` copies it to `web/pkg/`, with its flag), its routes in
   `server/openapi.json`, and the manifest's hashes for the two
   documents. There is no feed endpoint (D-124).

## Done when

- A Go round trip writes an alert and a template, reads each log, and
  is refused a write that `validate` refuses.
- **A real-data check.** Decode the existing app's feed
  (`gtfs-rt.c-tran.com/Alerts`) and write its alerts as authored
  records over the C-TRAN network. Encode them and decode them again.
  The live document gives the same alerts as the existing feed. The
  same path brings the existing alerts across when the agency moves to
  the new page.
- The docs describe it: server.md, storage.md, agency-data.md,
  FORMATS.md (the logs), and DEPLOYMENTS-SERVER.md (the new module
  flag).

The page, its e2e spec, and its dogfood session are
[ui-disruptions.md](ui-disruptions.md)'s.
