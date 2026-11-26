// The broken fixtures of tools/gates/crate-laws.mjs.

import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { exportOutsideHomes, nondeterminism } from "./crate-laws.mjs";

function tree(entries) {
  const root = mkdtempSync(join(tmpdir(), "crate-laws-"));
  for (const [path, text] of Object.entries(entries)) {
    mkdirSync(dirname(join(root, path)), { recursive: true });
    writeFileSync(join(root, path), text);
  }
  return root;
}

test("the whole-feed write outside the codec's homes fails", () => {
  const root = tree({
    "crates/transit-codec/src/lib.rs": "pub fn export() {}",
    "crates/cap-network/src/lib.rs": "use transit_codec::export;",
    "crates/generator/src/lib.rs": "transit_codec::export(&n);",
    "crates/cap-board/src/lib.rs": "use transit_codec::{read, export};",
    "crates/transit-runs/tests/t.rs": "transit_codec::export(&n);",
  });
  try {
    const problems = exportOutsideHomes(root);
    assert.equal(problems.length, 2);
    assert.match(problems[0], /^crates\/cap-board\/src\/lib\.rs:1 /);
    assert.match(problems[1], /^crates\/transit-runs\/tests\/t\.rs:1 /);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a clock, randomness, or the environment in library code fails; hosts may read them", () => {
  const root = tree({
    "crates/transit-runs/src/lib.rs": "fn ok() {}",
    "crates/transit-runs/src/deep/clock.rs": "let t = std::time::SystemTime::now();",
    "crates/cap-board/src/lib.rs": "let v = std::env::var(\"X\");",
    "crates/cap-abi/src/bin/gen.rs": "let v = std::env::var(\"X\");",
    "crates/generator/src/lib.rs": "let t = Instant::now();",
    "crates/transit-runs/tests/t.rs": "let r = rand::random();",
  });
  try {
    const problems = nondeterminism(root);
    assert.deepEqual(problems.map((p) => p.split(" ")[0]), ["crates/cap-board/src/lib.rs:1", "crates/transit-runs/src/deep/clock.rs:1"]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
