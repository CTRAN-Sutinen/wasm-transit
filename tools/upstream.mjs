// caps-run's upstream (docs/architecture/hosts.md): the server or an
// export folder, read on the paths a page reads. A head (`manifest`) is
// read on every command. A blob (`blobs/<hash>.gz`) is read once and kept
// in the working copy, because a copy by hash is never old.
// upstream.test.mjs runs the host contract suite against readHead.

import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";

export const LIMIT_MS = 10_000;

export class ReadFailed extends Error {}

// The head the host serves. `fetchHead` answers {text, asOfMs}, where
// asOfMs is the server's own age (X-Store-As-Of) or null, or it throws.
// `limit` rejects when the time limit passes. `kept` holds the last
// answer. The age is the upstream's last answer, never this read.
export async function readHead({ fetchHead, limit, nowMs, kept }) {
  let answer = null;
  let why = null;
  try {
    answer = await Promise.race([fetchHead(), limit]);
  } catch (e) {
    why = e?.message ?? String(e);
  }
  if (answer) {
    const asOfMs = answer.asOfMs ?? nowMs();
    kept.save({ text: answer.text, as_of_ms: asOfMs });
    return answer.asOfMs == null
      ? { state: "current", text: answer.text }
      : { state: "stale", text: answer.text, as_of_ms: asOfMs, why: "the server's store is not answering" };
  }
  const copy = kept.load();
  if (!copy) throw new ReadFailed(`the upstream's manifest could not be read (${why}), and the working copy holds none`);
  return { state: "stale", text: copy.text, as_of_ms: copy.as_of_ms, why: `the upstream did not answer (${why})` };
}

const isUrl = (upstream) => /^https?:\/\//.test(upstream);

// The working copy for one upstream: its last manifest, and its blobs by
// hash.
export function workingCopy(root, upstream) {
  const dir = join(root, ".upstreams", upstream.replace(/[^A-Za-z0-9._-]/g, "_"));
  const blobFile = (hash) => join(dir, "blobs", `${hash.replace(/[^A-Za-z0-9]/g, "_")}.gz`);
  const write = (file, bytes) => {
    mkdirSync(join(file, ".."), { recursive: true });
    writeFileSync(`${file}.tmp`, bytes);
    renameSync(`${file}.tmp`, file);
  };
  return {
    dir,
    head: {
      load: () => (existsSync(join(dir, "manifest.json")) ? JSON.parse(readFileSync(join(dir, "manifest.json"), "utf8")) : null),
      save: (copy) => write(join(dir, "manifest.json"), JSON.stringify(copy)),
    },
    blob: (hash) => (existsSync(blobFile(hash)) ? readFileSync(blobFile(hash)) : null),
    keepBlob: (hash, bytes) => write(blobFile(hash), bytes),
  };
}

function limitAfter(ms) {
  return new Promise((_, reject) => setTimeout(() => reject(new Error(`no answer in ${ms} ms`)), ms).unref());
}

// The manifest, current or stale with its age.
export async function manifest(upstream, copy, { limitMs = LIMIT_MS } = {}) {
  const fetchHead = isUrl(upstream)
    ? async () => {
        const resp = await fetch(new URL("manifest", upstream.endsWith("/") ? upstream : `${upstream}/`), { signal: AbortSignal.timeout(limitMs) });
        if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
        const stale = resp.headers.get("X-Store-Stale") === "true";
        const asOf = Number(resp.headers.get("X-Store-As-Of"));
        return { text: await resp.text(), asOfMs: stale ? (Number.isFinite(asOf) && asOf > 0 ? asOf : 0) : null };
      }
    : async () => {
        const file = join(upstream, "manifest");
        if (!existsSync(file)) throw new Error(`${file} does not exist`);
        return { text: readFileSync(file, "utf8"), asOfMs: null };
      };
  const head = await readHead({ fetchHead, limit: limitAfter(limitMs), nowMs: Date.now, kept: copy.head });
  return { ...head, manifest: JSON.parse(head.text) };
}

// A blob's gzipped bytes, from the working copy or else the upstream.
export async function blob(upstream, copy, hash, { limitMs = LIMIT_MS } = {}) {
  const kept = copy.blob(hash);
  if (kept) return kept;
  let bytes;
  if (isUrl(upstream)) {
    const resp = await fetch(new URL(`blobs/${hash}.gz`, upstream.endsWith("/") ? upstream : `${upstream}/`), { signal: AbortSignal.timeout(limitMs) });
    if (resp.status === 404) return null;
    if (!resp.ok) throw new Error(`blobs/${hash}.gz: HTTP ${resp.status}`);
    bytes = Buffer.from(await resp.arrayBuffer());
  } else {
    const file = join(upstream, "blobs", `${hash}.gz`);
    if (!existsSync(file)) return null;
    bytes = readFileSync(file);
  }
  copy.keepBlob(hash, bytes);
  return bytes;
}
