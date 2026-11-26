import assert from "node:assert/strict";
import test from "node:test";
import { check } from "./deps-gate.mjs";

const allow = `
[module]
serde   the documents' encoding
[server]
github.com/tetratelabs/wazero   runs the modules
`;

test("a tree equal to its list passes", () => {
  assert.deepEqual(check(allow, { module: ["serde"], server: ["github.com/tetratelabs/wazero"] }), []);
});

test("a new dependency fails until it is listed", () => {
  const p = check(allow, { module: ["serde", "chrono"], server: ["github.com/tetratelabs/wazero"] });
  assert.equal(p.length, 1);
  assert.match(p[0], /\[module\] chrono is a dependency that tools\/dependencies.txt does not list/);
});

test("a listed dependency the tree dropped fails", () => {
  const p = check(allow, { module: [], server: ["github.com/tetratelabs/wazero"] });
  assert.match(p.join("\n"), /\[module\] serde is listed, but the tree no longer has it/);
});

test("an entry with no reason fails", () => {
  const p = check("[pages]\nmaplibre-gl\n", { pages: ["maplibre-gl"] });
  assert.match(p.join("\n"), /maplibre-gl gives no reason/);
});

test("a tree with no section lists nothing", () => {
  const p = check(allow, { pages: ["pmtiles"] });
  assert.match(p.join("\n"), /\[pages\] pmtiles is a dependency/);
});
