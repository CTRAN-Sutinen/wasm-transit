// Runs web/src/uis/host-contract.json against caps-run's head read
// (docs/architecture/hosts.md, "The rules every host keeps"). The server
// and the browser run the same scenarios, and each host must give the
// same state for each step.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readHead, ReadFailed } from "./upstream.mjs";

const contract = JSON.parse(readFileSync(new URL("../web/src/uis/host-contract.json", import.meta.url), "utf8"));
const LIMIT_MS = 10_000;

for (const scenario of contract.scenarios) {
  test(`caps-run: ${scenario.name}`, async () => {
    let now = 1_000_000;
    let lastAnswerAt = null;
    let copy = null;
    const kept = { load: () => copy, save: (c) => void (copy = c) };
    for (const [i, step] of scenario.steps.entries()) {
      // Each command reads the head once, a poll's interval after the last.
      now += 30_000;
      const where = `${scenario.name}, step ${i + 1}`;
      let fetchHead;
      let limit = new Promise(() => {});
      if (step.upstream === "answers") {
        lastAnswerAt = now;
        fetchHead = async () => ({ text: step.head, asOfMs: null });
      } else {
        // Silent never answers, and slow answers after the limit: either
        // way the limit passes first.
        const head = step.head;
        fetchHead = () => (step.upstream === "slow" ? new Promise((r) => setTimeout(() => r({ text: head, asOfMs: null }), 50)) : new Promise(() => {}));
        limit = Promise.reject(new Error(`no answer in ${LIMIT_MS} ms`));
      }
      let seen;
      try {
        seen = await readHead({ fetchHead, limit, nowMs: () => now, kept });
      } catch (e) {
        assert.ok(e instanceof ReadFailed, where);
        seen = { state: "failed" };
      }
      assert.equal(seen.state, step.expect.state, where);
      if (step.expect.head) assert.equal(seen.text, step.expect.head, where);
      if (step.expect.as_of === "last-answer") assert.equal(seen.as_of_ms, lastAnswerAt, `${where}: the age is the upstream's last answer`);
    }
  });
}

test("caps-run: the server's own age passes down the chain", async () => {
  let copy = null;
  const kept = { load: () => copy, save: (c) => void (copy = c) };
  const seen = await readHead({ fetchHead: async () => ({ text: "A", asOfMs: 5_000 }), limit: new Promise(() => {}), nowMs: () => 9_000, kept });
  assert.deepEqual([seen.state, seen.as_of_ms], ["stale", 5_000]);
  const later = await readHead({ fetchHead: () => new Promise(() => {}), limit: Promise.reject(new Error("silent")), nowMs: () => 20_000, kept });
  assert.equal(later.as_of_ms, 5_000);
});
