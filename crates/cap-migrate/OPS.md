# migrate

Bring an older document forward to the version this build reads. Every authored kind's chain is kept forever (D-66) in the library that owns the kind — one pure step per version, every record id kept, every loss reported as a note naming its step — and this capability wraps them, so a consumer reads the current version only and refuses any other at its load. Assemble it where an older document arrives: the server's receive, a browser's restore, a file an agent loads. Migration never destroys: the answer is a new document under its own hash, and the caller keeps the old bytes.

## migrate

The document of kind ("network", "edit_records", "watchlist") brought to version `to` (omitted: the version this build reads) through every step between: the migrated document, its canonical bytes and document_hash (the identity every module registers it under), from, to, and notes — every loss, each naming the step it came from. A network without schema_version is v1 (every untagged shape, pre-trip and pre-alignment included); a bare edit array is v1. Refused: a document already at or past `to` (nothing to migrate), a document newer than this build (naming both versions — never re-emitted), an unknown kind (naming the kinds it knows).

## version

The version a document of kind is in, and the version this build reads — whether it needs migrate, without migrating it.
