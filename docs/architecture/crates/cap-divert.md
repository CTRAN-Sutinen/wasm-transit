# cap-divert

The operator-edit authoring capability: what does this operator edit amount to over the network? An operator edit is a diversion or a trip cancel (D-102).

**Ops.**
- `draft` gives a diversion editor's whole view.
- `list` gives the edit document's rows with names resolved: diversions in `rows`, trip cancels in `cancels`.
- `reports` gives what each record amounts to on the network: valid, or what is wrong, and the trip instances it touches. It reports invalid records and never rejects them. A day view patches nothing for them.
- `propose`, `cancel`, `retire`, and `restore` give the edit document after the change. They follow the envelope rules. `cancel` refuses an unknown trip, an instance it never runs, and a date it does not run.

**Contract.**
- Every op takes the edit document whole (`edits`) and answers the next one tagged. So a host persists or posts the answer as given.
- The capability refuses another version, naming both (`cap_abi::Current`, the version check that every kernel load shares). `propose`, `cancel`, `retire`, and `restore` write: their edit document is a `cap_abi::Base`, which also refuses a copy read ahead read-only (D-186).
- Reading a received document is cap-sync's `read_edits`.
- This is a separate need from cap-sync's converge step. The Go server hosts that step without a network.
- The host passes the clock and the record ids.

No browser UI authors an edit yet. Drawing a diversion is `docs/units/ui-disruptions.md`'s question 4. Cancelling a trip is its question 6.
