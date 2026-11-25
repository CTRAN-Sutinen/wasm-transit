// Pins that no host code branches on an error's text
// (docs/architecture/bus.md, "Answers"). A host acts on a refusal's code
// (src/refusals.ts).
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SRC = path.dirname(fileURLToPath(import.meta.url));
// A matcher applied to an error, its message, or a module outcome.
const SUBJECT = String.raw`\b(?:e|err|error|outcome|reason|message|\w+\.message|\w+\.error)\b`;
const MATCHES = [
  new RegExp(SUBJECT + String.raw`\s*\.\s*(?:includes|startsWith|endsWith|match|search)\(`),
  new RegExp(String.raw`\.(?:exec|test)\(\s*` + SUBJECT),
];

export function textMatches(source) {
  const hits = [];
  source.split("\n").forEach((line, i) => {
    if (/^\s*(\/\/|\*)/.test(line)) return;
    if (MATCHES.some((m) => m.test(line))) hits.push(`${i + 1}: ${line.trim()}`);
  });
  return hits;
}

function files(dir) {
  return readdirSync(dir).flatMap((name) => {
    const full = path.join(dir, name);
    if (statSync(full).isDirectory()) return name === "types" ? [] : files(full);
    return /\.(ts|svelte)$/.test(name) && !/\.test\./.test(name) ? [full] : [];
  });
}

test("1. the gate finds a host that matches an error's text", () => {
  assert.equal(textMatches(`if (error.message.includes("newer build")) update();`).length, 1);
  assert.equal(textMatches(`const m = /unknown (sha256:\\w+)/.exec(error.message);`).length, 1);
  assert.equal(textMatches(`if (refused.error.code === "newer_version") update();`).length, 0);
});

test("2. no host code in web/src matches an error's text", () => {
  const found = files(SRC).flatMap((f) => textMatches(readFileSync(f, "utf8")).map((hit) => `${path.relative(SRC, f)}:${hit}`));
  assert.deepEqual(found, []);
});
