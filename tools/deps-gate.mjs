// DEPENDENCIES: each shipped tree's external dependencies equal
// tools/dependencies.txt, and each entry there has a reason. The trees:
// the module builds' crates, the server binary's Go modules, and the
// pages' run-time npm packages.
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export function parseAllow(text) {
  const trees = {};
  const problems = [];
  let tree = null;
  text.split("\n").forEach((raw, i) => {
    const line = raw.replace(/#.*/, "").trim();
    if (!line) return;
    const head = line.match(/^\[([a-z]+)\]$/);
    if (head) {
      tree = trees[head[1]] ??= new Map();
      return;
    }
    const [name, ...why] = line.split(/\s+/);
    if (!tree) problems.push(`line ${i + 1}: ${name} is under no [tree]`);
    else if (!why.length) problems.push(`line ${i + 1}: ${name} gives no reason`);
    else tree.set(name, why.join(" "));
  });
  return { trees, problems };
}

/// actual: { tree: string[] }. Answers each difference, in words.
export function check(allowText, actual) {
  const { trees, problems } = parseAllow(allowText);
  for (const [tree, names] of Object.entries(actual)) {
    const listed = trees[tree] ?? new Map();
    const have = new Set(names);
    for (const n of [...have].sort())
      if (!listed.has(n)) problems.push(`[${tree}] ${n} is a dependency that tools/dependencies.txt does not list: add it with its reason, or remove it`);
    for (const n of [...listed.keys()].sort())
      if (!have.has(n)) problems.push(`[${tree}] ${n} is listed, but the tree no longer has it: remove its line`);
  }
  return problems;
}

function moduleCrates(root) {
  const mods = readdirSync(join(root, "assemblies")).filter((d) => d.startsWith("module-"));
  const out = execFileSync(
    "cargo",
    ["tree", "--offline", "--target", "wasm32-unknown-unknown", "-e", "normal", "--prefix", "none", "--format", "{p}", ...mods.flatMap((m) => ["-p", m])],
    { cwd: root, encoding: "utf8" },
  );
  return [...new Set(out.split("\n").filter((l) => l && !l.includes("(/")).map((l) => l.split(" ")[0]))];
}

function serverModules(root) {
  const out = execFileSync("go", ["list", "-deps", "-f", "{{with .Module}}{{.Path}}{{end}}", "."], { cwd: join(root, "server"), encoding: "utf8" });
  return [...new Set(out.split("\n").filter((l) => l && l !== "rustwasm/server"))];
}

function pagePackages(root) {
  const lock = JSON.parse(readFileSync(join(root, "web/package-lock.json"), "utf8"));
  const names = new Set();
  for (const [path, p] of Object.entries(lock.packages)) {
    if (!path || p.dev || p.devOptional) continue;
    names.add(path.replace(/.*node_modules\//, ""));
  }
  return [...names];
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const root = join(dirname(fileURLToPath(import.meta.url)), "..");
  const problems = check(readFileSync(join(root, "tools/dependencies.txt"), "utf8"), {
    module: moduleCrates(root),
    server: serverModules(root),
    pages: pagePackages(root),
  });
  if (problems.length) {
    console.error(problems.join("\n"));
    process.exit(1);
  }
}
