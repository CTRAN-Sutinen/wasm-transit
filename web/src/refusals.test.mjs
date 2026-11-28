// Pins how a host reads a module error (docs/architecture/bus.md, "Answers"):
// by its code, whatever the message says.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  Refused,
  moduleError,
  newerVersion,
  unknownDocument,
} from "./refusals.ts";
import { noteRefusal, onNewerBuild } from "./versions.ts";

const HASH = "sha256:" + "a".repeat(64);
const unknown = {
  message: "any words at all",
  code: "unknown_document",
  kind: "network",
  hash: HASH,
  load: "load_network",
};
const newer = {
  message: "reworded",
  code: "newer_version",
  kind: "edit_records",
  found: 3,
  reads: 2,
  read: false,
};

test("1. the engine finds an evicted document by its code, whatever the message says", () => {
  assert.equal(unknownDocument(new Refused(unknown)), HASH);
  assert.equal(
    unknownDocument(
      new Refused({ ...unknown, message: "something else entirely" }),
    ),
    HASH,
  );
});

test("2. a message that reads like a refusal but has no code asks for nothing", () => {
  const old = { message: `unknown network ${HASH}: load_network first` };
  assert.equal(unknownDocument(new Refused(old)), null);
  assert.equal(
    newerVersion(
      new Refused({
        message: "edit_records v3 is newer than this module, which reads v2",
      }),
    ),
    null,
  );
});

test("3. a newer version names its kind and the version found, read or not", () => {
  assert.deepEqual(newerVersion(new Refused(newer)), {
    kind: "edit_records",
    version: 3,
  });
  assert.deepEqual(newerVersion(new Refused({ ...newer, read: true })), {
    kind: "edit_records",
    version: 3,
  });
});

test("4. the server's answer to a refused write carries the same error", () => {
  const body = JSON.stringify({ error: newer });
  assert.deepEqual(moduleError(body), newer);
  assert.equal(moduleError("plain text from a proxy"), null);
  assert.equal(moduleError('{"error": not json'), null);
});

test("5. a newer version, and nothing else, asks for the deployment's build", () => {
  let asked = 0;
  const off = onNewerBuild(() => asked++);
  noteRefusal(new Refused({ message: "invalid document" }));
  noteRefusal(new Refused(unknown));
  noteRefusal(JSON.stringify({ error: newer }));
  noteRefusal(new Refused(newer));
  off();
  noteRefusal(new Refused(newer));
  assert.equal(asked, 2);
});
