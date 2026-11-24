// The target prune (`just prune`; `just test` runs it last): it deletes
// the build artifacts in target/ that the gate's own cargo commands no
// longer use.
//
// Cargo names each artifact by a hash of its inputs: the compiler, the
// profile, the features, and the hashes of its dependencies. When one
// of them changes, cargo writes new files beside the old ones and never
// deletes the old ones. Three things change them here:
//
// - A `-p` run. Cargo unifies features only across the packages that
//   one command selects (`resolver.feature-unification = "workspace"`
//   is still unstable in cargo 1.100), so `cargo test -p <crate>` and
//   the gate's `cargo test` build different copies of a shared
//   dependency, and of every crate above it.
// - A change to the dependency graph (a dependency or a feature added
//   to a crate).
// - A toolchain upgrade (rust-toolchain.toml). Every artifact goes
//   stale at once; `cargo clean` is the cheaper step there.
//
// The prune asks cargo for the live set: it runs the gate's cargo
// commands again with `--message-format=json`, which lists every
// artifact that each command uses, fresh or rebuilt. Right after the
// gate, every unit is fresh, so this takes seconds. Run alone on a
// changed tree, it builds what the gate builds first. If one of the
// commands fails, the prune deletes nothing.
//
// It then deletes each entry in deps/, build/, and .fingerprint/ whose
// hash is not in the live set, in the profile directories the gate
// writes: target/debug, target/wasm-fast (the host side of the module
// build: build scripts and proc macros), and
// target/wasm32-unknown-unknown/wasm-fast. The release directories
// (`just caps`, `just test-release`) are not touched.
//
// incremental/ names its directories by another hash, which cargo does
// not report. For each crate name, the prune keeps the newest N
// directories, where N is the number of live units with that name (a
// library and its unit tests are two). A live unit whose cache is
// deleted compiles once without it.
//
// After the gate, the next `cargo test -p <crate>` builds its own copy
// again. That is the cost of a bounded target/.
//
// Usage: node tools/prune-target.mjs [--dry-run]
// Do not run it while another cargo command builds into target/.

import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, rmSync, statSync } from "node:fs";
import { join } from "node:path";

const dryRun = process.argv.includes("--dry-run");
const root = new URL("..", import.meta.url).pathname;
const target = join(root, "target");
const HASH = /-([0-9a-f]{16})(?=[./]|$)/g;

if (!existsSync(target)) {
  console.log("prune: no target/ to prune");
  process.exit(0);
}

// One cargo command of the gate, run again for its artifact list.
function query(args, env = {}) {
  const r = spawnSync("cargo", [...args, "--message-format=json"], {
    cwd: root,
    env: { ...process.env, ...env },
    encoding: "utf8",
    maxBuffer: 1 << 28,
  });
  if (r.status !== 0) {
    process.stderr.write(r.stderr);
    console.error(`prune: \`cargo ${args.join(" ")}\` failed; nothing deleted`);
    process.exit(1);
  }
  return r.stdout
    .split("\n")
    .filter((l) => l.startsWith("{"))
    .map((l) => JSON.parse(l));
}

const liveHashes = new Set();
const liveUnits = new Map(); // profile dir -> Map(crate name -> live unit count)
function record(messages, profileDir) {
  const units = liveUnits.get(profileDir) ?? new Map();
  liveUnits.set(profileDir, units);
  for (const m of messages) {
    const paths = [];
    if (m.reason === "compiler-artifact") {
      paths.push(...m.filenames);
      const name = m.target.name.replaceAll("-", "_");
      units.set(name, (units.get(name) ?? 0) + 1);
    } else if (m.reason === "build-script-executed") {
      paths.push(m.out_dir);
    }
    for (const p of paths) for (const [, h] of p.matchAll(HASH)) liveHashes.add(h);
  }
}

// The host side: `_host-tests` (`cargo test`), `just types` (`cargo
// test --workspace export_bindings`, the same selection), and the
// generator binaries that `just caps` and the arch gates run with `-p`.
record(query(["test", "--no-run"]), "debug");
record(query(["build", "-p", "cap-abi", "--bins"]), "debug");
if (existsSync(join(root, "assemblies/gen-manifest"))) {
  record(query(["build", "-p", "gen-manifest"]), "debug");
}

// The wasm-fast modules: `_modules` builds each assembly alone, with
// its RUSTFLAGS, and so does this.
const wasmFast = join(target, "wasm32-unknown-unknown/wasm-fast");
if (existsSync(wasmFast) && existsSync(join(root, "assemblies"))) {
  const home = process.env.CARGO_HOME ?? join(process.env.HOME, ".cargo");
  const env = {
    RUSTFLAGS: `--remap-path-prefix=${home}=/cargo --remap-path-prefix=${root.replace(/\/$/, "")}=/src`,
    CARGO_TARGET_DIR: "target",
  };
  for (const crate of readdirSync(join(root, "assemblies")).filter((d) => d.startsWith("module-")).sort()) {
    const msgs = query(
      ["build", "--locked", "--profile", "wasm-fast", "-p", crate, "--target", "wasm32-unknown-unknown"],
      env,
    );
    record(msgs, "wasm32-unknown-unknown/wasm-fast");
    record(msgs, "wasm-fast");
  }
}

function size(p) {
  const s = statSync(p);
  if (!s.isDirectory()) return s.size;
  return readdirSync(p).reduce((n, f) => n + size(join(p, f)), 0);
}

let freed = 0;
let removed = 0;
function remove(p) {
  freed += size(p);
  removed += 1;
  if (!dryRun) rmSync(p, { recursive: true, force: true });
}

for (const profileDir of ["debug", "wasm-fast", "wasm32-unknown-unknown/wasm-fast"]) {
  const dir = join(target, profileDir);
  if (!existsSync(dir)) continue;
  for (const sub of ["deps", "build", ".fingerprint"]) {
    const d = join(dir, sub);
    if (!existsSync(d)) continue;
    for (const f of readdirSync(d)) {
      const hashes = [...f.matchAll(HASH)].map(([, h]) => h);
      if (hashes.length > 0 && !hashes.some((h) => liveHashes.has(h))) remove(join(d, f));
    }
  }
  const inc = join(dir, "incremental");
  if (!existsSync(inc)) continue;
  const units = liveUnits.get(profileDir) ?? new Map();
  const byName = new Map();
  for (const f of readdirSync(inc)) {
    const name = f.slice(0, f.lastIndexOf("-"));
    byName.set(name, [...(byName.get(name) ?? []), { f, t: statSync(join(inc, f)).mtimeMs }]);
  }
  for (const [name, dirs] of byName) {
    dirs.sort((a, b) => b.t - a.t);
    for (const { f } of dirs.slice(units.get(name) ?? 0)) remove(join(inc, f));
  }
}

const gb = (freed / 1e9).toFixed(1);
console.log(`prune: ${dryRun ? "would free" : "freed"} ${gb} GB (${removed} entries)`);
