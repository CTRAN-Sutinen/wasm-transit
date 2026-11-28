// Runs web/src/uis/host-contract.json (docs/architecture/hosts.md, "The
// rules every host keeps"). The server runs the same scenarios in
// server/hostcontract_test.go and caps-run in tools/upstream.test.mjs,
// and every host must give the same state
// for each step.

import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { inputs, labelOf } from "./inputs.ts";
import { headAnswer } from "./heads.ts";

const contract = JSON.parse(
  fs.readFileSync(new URL("./host-contract.json", import.meta.url), "utf8"),
);

const EVERY_S = 30;
const LIMIT_S = 10;
const settle = () => new Promise((r) => setTimeout(r, 0));

function host() {
  let now = 1_000_000;
  let timers = [];
  let seq = 0;
  const h = {
    states: {},
    now: () => now,
    random: () => 0,
    after: (ms, fn) => {
      const t = { at: now + ms, fn, id: seq++ };
      timers.push(t);
      return () => (timers = timers.filter((x) => x !== t));
    },
    isVisible: () => true,
    onVisible: () => {},
    request: async () => {},
  };
  h.advance = async (ms) => {
    const until = now + ms;
    for (;;) {
      await settle();
      timers.sort((a, b) => a.at - b.at || a.id - b.id);
      const next = timers[0];
      if (!next || next.at > until) break;
      timers.shift();
      now = next.at;
      // A poll's tick waits on its reads, so the clock runs each timer and
      // moves on.
      void next.fn();
    }
    now = until;
    await settle();
  };
  return h;
}

function seen(state) {
  if (state.kind === "answered")
    return state.asOfMs == null ? "current" : "stale";
  if (state.kind === "failed") return state.held ? "stale" : "failed";
  return state.kind;
}

for (const scenario of contract.scenarios) {
  test(`the browser: ${scenario.name}`, async () => {
    const h = host();
    const upstream = { mode: "silent", head: "" };
    let lastAnswerAt = null;
    let held = null;
    const reader = {
      read: () => {
        if (upstream.mode === "answers") {
          lastAnswerAt = h.now();
          return Promise.resolve(
            headAnswer(
              { status: 200, header: () => null, text: upstream.head },
              [],
              h.now(),
            ),
          );
        }
        if (upstream.mode === "slow") {
          const head = upstream.head;
          return new Promise((resolve) =>
            h.after((LIMIT_S + 5) * 1000, () =>
              resolve(
                headAnswer(
                  { status: 200, header: () => null, text: head },
                  [],
                  h.now(),
                ),
              ),
            ),
          );
        }
        return new Promise(() => {});
      },
      adopt: (value) => void (held = value),
    };
    const run = inputs(
      [
        {
          id: "edits",
          name: "the detours",
          from: "GET edits",
          every_s: EVERY_S,
          limit_s: LIMIT_S,
          keeps: "last",
          feeds: ["edits"],
          fails_as: [],
        },
      ],
      { edits: reader },
      h,
    );
    for (const [i, step] of scenario.steps.entries()) {
      upstream.mode = step.upstream;
      if (step.head) upstream.head = step.head;
      if (i === 0) {
        const started = run.start();
        await h.advance(LIMIT_S * 1000 + 1);
        await started;
      } else {
        await h.advance(EVERY_S * 1000 + LIMIT_S * 1000 + 1);
      }
      const state = h.states.edits;
      const where = `${scenario.name}, step ${i + 1}`;
      assert.equal(seen(state), step.expect.state, where);
      if (step.expect.head) assert.equal(held, step.expect.head, where);
      if (step.expect.as_of === "last-answer")
        assert.equal(
          state.asOfMs,
          lastAnswerAt,
          `${where}: the age is the upstream's last answer`,
        );
      const label = labelOf(state, (ms) => `t${ms}`);
      if (step.expect.state === "current") assert.equal(label, "", where);
      if (step.expect.state === "stale")
        assert.match(
          label,
          new RegExp(`^Detours as of t${state.asOfMs}`),
          where,
        );
      if (step.expect.state === "failed")
        assert.match(label, /^Detours could not be read/, where);
    }
  });
}
