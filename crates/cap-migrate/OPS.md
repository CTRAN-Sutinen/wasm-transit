# migrate

Bring an older document forward to the version this build reads.

The library that owns each authored kind keeps that kind's chain forever. A chain has one pure step for each version. Every record id is kept. Every loss is reported as a note that names its step. This capability wraps the chains. So a consumer reads the current version only, and it refuses any other version at its load.

Assemble this capability where an older document arrives: the server's receive, a browser's restore, or a file that an agent loads.

Migration never destroys. The answer is a new document under its own hash, and the caller keeps the old bytes.

## migrate

Bring a document of a given kind to version `to`, through every step between. The kinds are `network`, `edit_records`, and `watchlist`. When `to` is omitted, it is the version this build reads.

The answer holds these parts:

- the migrated document;
- its canonical bytes, and `document_hash`, which is the identity that every module registers it under;
- `from` and `to`;
- `notes`: every loss, each naming the step it came from.

Two untagged shapes have a fixed version. A network without `schema_version` is v1. This covers every untagged shape, including pre-trip and pre-alignment ones. A bare edit array is v1.

The op refuses in these cases:

- A document already at or past `to`: there is nothing to migrate.
- A document newer than this build: the error names both versions, and the op never re-emits the document.
- An unknown kind: the error names the kinds it knows.

## version

The version that a document of a given kind is in, and the version this build reads. The answer tells whether the document needs `migrate`, without migrating it.
