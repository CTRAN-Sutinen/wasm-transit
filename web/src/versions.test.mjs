// A refusal of a document another build wrote (src/versions.ts): the
// two refusal texts transit-version writes, and nothing else, ask for
// the deployment's build.
//
// Run: node --test web/src/versions.test.mjs (`just test`, web gates).
import { test } from "node:test";
import assert from "node:assert/strict";
import { newerRefusal, noteRefusal, onNewerBuild } from "./versions.ts";

test("a newer version and a write refused for its version name the kind and version", () => {
  assert.deepEqual(
    newerRefusal("edit_records v3 is newer than this module, which reads v2: a newer build wrote it (refused, never rewritten)"),
    { kind: "edit_records", version: 3 },
  );
  assert.deepEqual(
    newerRefusal("400 network v3 was written by a newer build: this build reads it one version ahead of v2, and does not write over it"),
    { kind: "network", version: 3 },
  );
});

test("an older version and other errors ask for nothing", () => {
  assert.equal(newerRefusal("edit_records v1: this module reads v2 — bring it forward first (migrate.migrate)"), null);
  assert.equal(newerRefusal("unknown network sha256:ab: load_network first"), null);
  let asked = 0;
  const off = onNewerBuild(() => asked++);
  noteRefusal("invalid document: expected value");
  noteRefusal("watchlist v3 is newer than this module, which reads v2");
  off();
  noteRefusal("watchlist v3 is newer than this module, which reads v2");
  assert.equal(asked, 1);
});
