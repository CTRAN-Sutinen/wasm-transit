# Service alerts: bringing the existing alerts across

The service alerts document, the alert templates document, cap-alerts,
and cap-rt's `encode_alerts` are on the bus (transit-alerts, cap-alerts,
and transit-rt in `docs/architecture/crates.md`; D-206..D-211). The
server keeps both documents in the alerts log (`POST /service-alerts`,
`POST /alert-templates`; `docs/architecture/server.md`). What is left
is reading the existing app's feed into the document, and proving the
encode against the feed that riders' apps read today. The
work replaces the custom app that C-TRAN's communications staff use
(D-207).

The page is the disruptions UI ([ui-disruptions.md](ui-disruptions.md),
D-209). Serving the feed is not in this unit, and it may be outside
this repo. The bucket is open to read, so the alerts document's layout
in the bucket is the contract that a serving endpoint reads.

## Scope, in order

1. **Reading an Alerts feed into the document.** An op answers a
   service alerts document from a GTFS-rt Alerts feed over the network:
   every alert, with every translation, its entities in the network's
   ids, and a direction by the export's code. It is the inverse of
   `encode_alerts`, and it lives beside it in cap-rt (GTFS-rt's home).
   Its law: reading back an encoded document gives the published
   alerts not yet ended. It is the path that brings the existing app's
   alerts across.

## Done when

- **A real-data check.** Read the existing app's feed
  (`gtfs-rt.c-tran.com/Alerts`) into the document over the C-TRAN
  network, write it through `POST /service-alerts`, encode it, and
  decode it again. The live document gives the same alerts as the
  existing feed.

The page, its e2e spec, and its dogfood session are
[ui-disruptions.md](ui-disruptions.md)'s.
