# calendar

The calendar: which service keys run on a date, and the reverse: the dates on which one service type runs. Often several keys run at once, for example a regular pattern plus an exception-added service.

Both answers come from the effective day, not from the calendar rules alone. A key is not running when every trip of the key is skipped by edits, or when the model never could represent the key. This holds however the calendar reads.

The capability answers over the materialized day of the network document. The domain code lives in the shared transit-dispatch library.

## active_services

The service keys running on a date: the distinct service types that the effective day's trips run under, sorted.

The answer is departures-only when `edits_hash` is omitted. This is the day's first question: what is running today?

## service_dates

The dates on which one service type runs, within an inclusive YYYY-MM-DD window. This is the same question that `active_services` answers in the other direction, for example "when does 1-WKDY/31 run?".

The effective day decides each date. Edits apply when `edits_hash` names a registered document. A type whose every trip is skipped is not running, however its calendar reads.

An unknown type or an inverted window answers empty, never an error.
