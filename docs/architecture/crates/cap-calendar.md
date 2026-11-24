# cap-calendar

The calendar capability gives two reads of one relation:

- `active_services`: which service keys run today.
- `service_dates`: the reverse, the dates one service type runs, over
  an inclusive window.

Both read the effective day. A key whose every trip is skipped or
cannot be represented is not running, whatever its calendar says. This
is the same rule as the runcut view's hole. cap-calendar reads the same
documents and the same held day as cap-board. The domain code is in the
shared transit-dispatch library. The arch gate forbids a GTFS layer
here.
