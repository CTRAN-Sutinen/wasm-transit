// Two laws over the crates' source (crates/AGENTS.md). The dependency
// graph's laws are the arch-gates bin (crates/cap-abi/src/bin/arch_gates.rs).
//
// - Law 1b: the whole-feed write (transit_codec::export) is the codec's
//   publish path. Only the codec's homes, and the generator and its
//   fixtures (which compute the export's worked example), name it.
// - Determinism (docs/architecture/web.md, "The engine"): no clock,
//   randomness, or environment in a crate's library code. Hosts pass time
//   in, so an answer is a function of its inputs and the engine's journal
//   key is sound. Binaries (src/bin/) and the generator are hosts.
//
// crate-laws.test.mjs is the broken fixture.

import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { files, matching, report } from "./files.mjs";

const EXPORT_HOMES = ["transit-codec", "cap-network", "cap-abi", "generator", "fixtures"];

export function exportOutsideHomes(root) {
  const rs = files(root, "crates").filter((p) => p.endsWith(".rs") && !EXPORT_HOMES.includes(p.split("/")[1]));
  return matching(root, rs, /transit_codec::export|codec::\{[^}]*\bexport\b/).map(
    (h) => `${h.path}:${h.line} names transit_codec::export outside the codec's homes (no whole-feed writes in capabilities)`,
  );
}

export function nondeterminism(root) {
  const rs = files(root, "crates").filter((p) => {
    const [, crate, src] = p.split("/");
    return p.endsWith(".rs") && src === "src" && crate !== "generator" && !p.includes("/src/bin/");
  });
  return matching(root, rs, /SystemTime|Instant::now|Utc::now|Local::now|std::env::|env::var|thread_rng|rand::|getrandom|OsRng/).map(
    (h) => `${h.path}:${h.line} reads a clock, randomness, or the environment inside a module (hosts pass time in): ${h.text}`,
  );
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
  process.exitCode = report("CRATE LAW FAIL (tools/gates/crate-laws.mjs)", [...exportOutsideHomes(root), ...nondeterminism(root)]);
}
