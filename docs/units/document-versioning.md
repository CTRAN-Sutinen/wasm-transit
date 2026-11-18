# Document versioning — every persisted document migrates forward

There is no central database to run a schema change against. A
document lives in the server store, in every browser's IndexedDB,
in files `caps-run` loads, in a request body sent by a client on
last month's build, and in the URL a stop display was configured
with. So the version travels WITH the document, and the code that
reads it carries the migrations: a document is migrated forward,
by the crate that owns its type, at the boundary where it is
received, into a NEW document (its own hash). The old bytes stay
addressable; rollback is a pointer moved back, never an inverse
migration.

The machinery stands (`docs/FORMATS.md`, "Versions" — the inventory
of every persisted kind, the rollout order, the kernel-version rule,
and URLs as persisted data; `docs/architecture/crates.md`, cap-migrate and
transit-version): every kind in the inventory carries its kind and
version inside it, and each one's schema and example are frozen in the
schema lock and served at `/schemas/`; every kernel load and every
request field taking one refuses another version naming both
(`cap_abi::parse_versioned`, `cap_abi::Current`); the network's, the
edit document's and the watchlist's chains live in the owning
libraries and cap-migrate wraps them; the Go server brings an older
edit document forward at receive, and at startup commits forward an
older `edits` head and the older networks of the picks in use
(cap-plans `in_use`), republishing the plan set through cap-plans; the planners' pages and ride-now bring a stored
plan or watchlist forward at restore, keeping the old record, and
ride-now's device records are tagged documents in the browser's store;
cap-divert takes and answers the tagged edit document; a stop display
keeps a URL parameter it does not know; a release reads one version
ahead, lossless or read-only, and an operator can move a pointer back
(`store revert`). What is left:

## Scope

1. **Rolling a version back (D-79, D-186).** The reader of the next
   version, the write refusal, the page's request for the deployment's
   build, the emergency procedure (`store revert`), and the rules stand
   (`docs/FORMATS.md`, "Rolling a version back"). What is left:
   - A staff page refused for its version gets the deployment's build
     when it joins the update cycle (`offline-apps.md`, D-189).
   - cap-plans' published set reads only its own version. Its ops take
     the set as a plain typed field. They move to `cap_abi::Current`
     and `cap_abi::Base` with the set's first format change.

## Done when

- Go and e2e: a store written by release B (vN+1) is read by release
  A (reads vN and vN+1) with no document lost or misread. This needs a
  real vN+1, so it lands with the first format change after the
  baseline. The laws over fixture kinds stand (cap-abi
  `tests/read_ahead_test.rs`, the kernel's read-ahead test).
