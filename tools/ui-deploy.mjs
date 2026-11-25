// The deploy tool for a UI at its own URL (D-171;
// docs/architecture/release-artifacts.md, "Each UI at its own URL").
// It writes one of the UI's logged builds out to a directory, for the
// UI's host to serve. The server keeps every build of the UI on the
// store's build.<id> pointer (-ui-dist). By default the tool writes the
// head. A rollback moves the pointer back (POST /builds?ui=<id>), then
// runs this tool again.
//
//   node tools/ui-deploy.mjs <server URL> <ui id> <out dir> [sha256:<build>]
//
// It reads only what every deployment answers openly: the pointer's
// head, then the build document and each file by hash
// (/pointers/build.<id>, /blobs/<hash>). It checks every file's bytes
// against its hash. OUT must be absent or empty, so a deploy never
// mixes two builds. A named build must be in the UI's log.

import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";

const [server, ui, out, named] = process.argv.slice(2);
if (!server || !ui || !out) {
  console.error("usage: node tools/ui-deploy.mjs <server URL> <ui id> <out dir> [sha256:<build>]");
  process.exit(2);
}
if (!/^[a-z0-9-]{1,64}$/.test(ui)) {
  console.error(`ui-deploy: ${ui} is not a UI's id (a-z, 0-9, and -)`);
  process.exit(2);
}
const base = server.replace(/\/+$/, "");
const pointer = `build.${ui}`;

async function get(rel) {
  const resp = await fetch(`${base}/${rel}`);
  if (!resp.ok) throw new Error(`GET /${rel}: ${resp.status} ${(await resp.text()).trim()}`);
  return Buffer.from(await resp.arrayBuffer());
}

async function blob(hash) {
  const bytes = await get(`blobs/${hash}`);
  const got = `sha256:${crypto.createHash("sha256").update(bytes).digest("hex")}`;
  if (got !== hash) throw new Error(`blobs/${hash}: the bytes hash to ${got}`);
  return bytes;
}

async function main() {
  let build = named;
  if (build) {
    const log = JSON.parse(await get(`pointers/${pointer}/log`));
    if (!(log.entries ?? []).some((e) => e.to === build)) throw new Error(`the ${pointer} log holds no ${build}`);
  } else {
    build = JSON.parse(await get(`pointers/${pointer}`)).hash;
    if (!build) throw new Error(`${pointer} has no build: start the server with -ui-dist ${ui}=<dir>`);
  }
  const doc = JSON.parse(await blob(build));
  if (doc.kind !== "build" || doc.schema_version !== 1) throw new Error(`${build} is a ${doc.kind} v${doc.schema_version}, not a build v1`);

  if (fs.existsSync(out) && fs.readdirSync(out).length > 0) throw new Error(`${out} is not empty: deploy into an absent or empty directory`);
  const root = path.resolve(out);
  for (const file of doc.files) {
    const target = path.resolve(root, file.path);
    if (!target.startsWith(root + path.sep)) throw new Error(`the build names a path outside the bundle: ${file.path}`);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, await blob(file.hash));
  }
  console.log(`ui-deploy: ${ui} ${build} → ${out} (${doc.files.length} files)`);
}

main().catch((err) => {
  console.error(`ui-deploy: ${err.message}`);
  process.exit(1);
});
