# Fewer dependencies — code the agency can own

Every dependency is code the agency must keep building, patching, and
reading after the vendor is gone. The project keeps a dependency only
when the code we would write in its place is worse. The server talks to
object storage over plain HTTP, written on the Go standard library, for
two providers: S3 (and S3-compatible stores) and GCS. Azure Blob is not
a target (D-225). The wasm modules link only what they run.

Measured 2026-11-20: the server binary links 100 Go modules. The GCS
SDK brings 83 of them (gRPC, OpenTelemetry, Arrow, flatbuffers), the
Azure SDK about 26, and the AWS SDK about 8. `Cargo.lock` holds 255
crates. The pages ship four npm packages at run time.

## Scope

1. **The S3 client on the standard library.** It replaces
   `aws-sdk-go-v2` in `server/store/s3.go`. It implements the store's
   `Backend` (`server/store/backend.go`): `Get` with the object's
   metadata, `Stat`, `Create` as the conditional create
   (`If-None-Match: *`), `List` after a key, and `Delete`. SigV4 signs
   each request. Path-style
   addresses stay for S3-compatible stores. The credentials are the
   sources `DEPLOYMENTS-SERVER.md` names: `AWS_ACCESS_KEY_ID` and
   `AWS_SECRET_ACCESS_KEY`, and the role of the machine or pod (the
   EC2 instance metadata service, the ECS container credentials, and
   EKS web identity through STS). The guide names each source the
   client reads, and no other.
2. **The GCS client on the JSON API.** It replaces
   `cloud.google.com/go/storage` and `google.golang.org/api` in
   `server/store/gcs.go`. It implements the same five `Backend`
   methods: media download with the object's metadata, the object's
   attributes for `Stat`, the upload with `ifGenerationMatch=0` as
   `Create`, list with page tokens, and delete. The credentials are a service
   account's key file (`GOOGLE_APPLICATION_CREDENTIALS`, a signed JWT
   exchanged for a token, `crypto/rsa`) and the metadata server of the
   machine or pod. The token is cached until shortly before it
   expires.
3. **Azure Blob out.** Delete `server/store/azure.go`, the `azblob://`
   address, Azurite from `just store-dev`, and Azure from
   `just store-contract`. Sweep `azblob`, `Azure`, and `Azurite` from
   every doc (`AGENTS.md`, `DEPLOYMENTS-SERVER.md`, `docs/FORMATS.md`,
   `docs/architecture/storage.md`, `store-durability.md`'s contract
   table, the guide).
4. **The Rust build trimmed.**
   - Delete `wasm-bindgen` and `serde-wasm-bindgen` from the
     workspace's dependencies. No crate uses them.
   - `ts-rs` and `schemars` behind a feature that only `gen-manifest`
     and `just types` turn on. A module build compiles neither.
   - `zopfli` leaves, if zip allows it. zip 2.6's `deflate` feature
     brings `zopfli`, and its `deflate-flate2` feature alone does not
     turn on `flate2`. Check first whether the codec writes through
     `zopfli`. The codec's zip bytes stay the same (the fixture's
     hash).
   - A civil date type of our own in place of `chrono`
     (`NaiveDate`, adding days, the weekday, parse and format of
     "YYYY-MM-DD"). Laws first: property tests against `chrono` as a
     dev dependency, then `chrono` and `num-traits` leave.
   - `thiserror` written out by hand (three files).
   - `prost` stays. A hand-written GTFS-rt decoder is a project of its
     own, and it is not in this unit.
5. **`idb-keyval` written out.** Its few IndexedDB calls become one
   module in `web/src/` that the four users import.
6. **The line holds.** A gate in `just test` compares the external
   dependencies of each tree (the module build's crates, the server
   binary's Go modules, the pages' run-time npm packages) against an
   allowlist file in the repo. A new dependency is a reviewed edit to
   that file, with its reason beside it.

The order: 1 and 2 before 3, so a deployment always has a client to
move to. 4 to 6 are independent.

## Done when

- `just store-contract` passes against SeaweedFS and fake-gcs-server
  with the hand-written clients, and the Go tests that cover each
  credential source pass. `go list -deps` of the server names no
  `aws-sdk-go-v2`, `cloud.google.com`, `google.golang.org`, or
  `azure-sdk-for-go` module.
- The date type's property tests pass against `chrono`. `cargo tree`
  of a module build names no `chrono`, `zopfli`, `thiserror`, `ts-rs`,
  or `schemars`.
- `just reproducible` passes, and the codec's fixture zip keeps its
  hash.
- The allowlist gate runs in `just test`.
- The docs name S3 and GCS only, and `DEPLOYMENTS-SERVER.md` names
  each credential source the clients read.
