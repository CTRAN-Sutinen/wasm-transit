# Server resilience — no single server copy is a point of failure

A deployment with the server has no single point of failure in the
server itself. The bucket is the agency's record, and each server copy
is a working copy over it. The store already answers a bucket that
stops answering: a running copy serves its cached heads with a stale
label and refuses writes with `503` and `Retry-After` (D-119). The
page's side of a short outage is `offline-apps.md`. This unit is the
server's side: more than one copy, a copy restarted while the bucket
does not answer, and tests that keep each failure mode working.

## Scope

1. **More than one copy, proved.** `DEPLOYMENTS-SERVER.md` ("Run it
   behind a load balancer") tells the operator to run as many copies
   as they like on one bucket. No test proves it. Each log entry is a
   conditional create, and a copy that loses the race reads the log
   again (`server/caplog.go`), so the design allows it. The test runs
   two server processes on one store. Both write to the same log at
   the same time, and each copy then serves the same documents.
2. **A copy restarted while the bucket does not answer.** Today such a
   copy has nothing to serve, and `/readyz` answers `unavailable`. A
   copy keeps a local cache of the blobs it served, by hash, and boots
   on it read-only: it serves the last heads it confirmed, labelled
   stale, and refuses writes as a running copy does. The cache is a
   working copy. Nothing in it is written back to the bucket as the
   record, and losing it loses nothing.
3. **A fault harness.** The tests have a bucket that stops answering
   (`flakyBucket` in `server/degraded_test.go`, `flaky` in
   `server/store/degraded_test.go`). The harness adds:
   - an answer lost after the server kept the write;
   - a server copy stopped in the middle of a write;
   - a slow bucket.

   Each fault has a Go test that names what must hold.

## Done when

- Go: two server copies on one store write to the same log at the same
  time. Every write lands once, in one order, and both copies serve the
  same heads.
- Go: a copy booted while the bucket does not answer serves the heads
  it last confirmed, labelled stale, and refuses writes with `503`.
- Go: for each fault in the harness, a test shows that no kept write
  is lost or written twice, and that the copy recovers when the fault
  ends.
- `docs/architecture/server.md` and `docs/architecture/storage.md`
  describe the cache and more than one copy. `DEPLOYMENTS-SERVER.md`
  says what `/readyz` answers for a copy that boots on its cache.
