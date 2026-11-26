// The broken fixtures of tools/gates/sessions.mjs: a wrong answer, a
// failed command, and a missing named result each fail the session, and
// the failure names the session and the script.

import { test } from "node:test";
import assert from "node:assert/strict";
import { caps, has, must, runSessions, SESSIONS } from "./sessions.mjs";

async function failures(sessions) {
  const lines = [];
  const failed = await runSessions(sessions, (line) => lines.push(line));
  return { failed, lines };
}

test("a wrong answer fails, named", async () => {
  const { failed, lines } = await failures([
    ["gtfs: the wrong feed version", () => has(caps(["gtfs", "--load", "feed:web/public/caps/sample-gtfs.zip", "--op", '{"op":"feed_info","feed_hash":"$load.feed_hash"}']).stdout, '"feed_version":"1999.1.1"')],
  ]);
  assert.equal(failed, 1);
  assert.match(lines[0], /^SESSION GATE FAIL \(tools\/gates\/sessions\.mjs\): gtfs: the wrong feed version: the answer lacks/);
});

test("a command that fails, fails the session", async () => {
  const { failed, lines } = await failures([["no such capability", () => caps(["nothing", "--op", '{"op":"x"}'])]]);
  assert.equal(failed, 1);
  assert.match(lines[0], /no such capability: caps-run exited/);
});

test("a result that is not there fails the session", async () => {
  const { failed } = await failures([["no result named x", () => caps(["plans", "--op", '{"op":"periods","plans":[]}']).named("x")]]);
  assert.equal(failed, 1);
});

test("every session continues past a failure, and each is named once", async () => {
  const { failed, lines } = await failures([
    ["first", () => must(false, "no")],
    ["second", () => {}],
    ["third", () => must(false, "no")],
  ]);
  assert.equal(failed, 2);
  assert.deepEqual(lines.map((l) => l.split(": ")[1]), ["first", "third"]);
  const names = SESSIONS.map(([name]) => name);
  assert.equal(new Set(names).size, names.length);
});
