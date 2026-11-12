# calendar

The calendar: which service keys run on a date — often several at once (a regular pattern plus an exception-added service) — and its reverse, the dates one service type runs on. Both answers come from the EFFECTIVE day, not the calendar rules alone: a key whose every trip is skipped by edits, or was never representable in the model, is not running however its calendar reads. The materialized day over the NETWORK document; the domain code lives in the shared transit-dispatch library.

## active_services

The service keys running on a date: the distinct service types the effective day's trips run under, sorted. Departures-only when edits_hash is omitted. The day's first question — what is running today?

## service_dates

The dates one service type runs on within an inclusive YYYY-MM-DD window — the same question active_services answers in the other direction ("when does 1-WKDY/31 run?"). The effective day decides each date (edits applied when edits_hash names a registered document): a type whose every trip is skipped is not running, however its calendar reads. An unknown type or an inverted window answers empty, never an error.
