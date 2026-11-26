// caps-run's upstream (docs/architecture/hosts.md): the server or an
// export folder, read on the paths a page reads. A head (`manifest`,
// `edits`, `network`) is read on every command that names it. A blob
// (`blobs/<hash>.gz`) is read once and kept in the working copy, because
// a copy by hash is never old. upstream.test.mjs runs the host contract
// suite against readHead.

import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";

export const LIMIT_MS = 10_000;

// The heads the pages read (each UI's ui.json, `inputs`), by the path
// that answers them.
export const HEADS = ["manifest", "edits", "network"];

export class ReadFailed extends Error {}

// The head the host serves. `fetchHead` answers {text, asOfMs, etag},
// where text is null for a head the upstream does not have (404),
// asOfMs is the server's own age (X-Store-As-Of) or null, and
// `notModified` means the kept copy is current (304). Or it throws.
// `limit` rejects when the time limit passes. `kept` holds the last
// answer. The age is the upstream's last answer, never this read.
export async function readHead({ name = "manifest", fetchHead, limit, nowMs, kept }) {
  let answer = null;
  let why = null;
  const copy = kept.load();
  try {
    answer = await Promise.race([fetchHead(copy?.etag ?? null), limit]);
  } catch (e) {
    why = e?.message ?? String(e);
  }
  if (answer) {
    const text = answer.notModified ? copy.text : answer.text;
    const asOfMs = answer.asOfMs ?? nowMs();
    kept.save({ text, as_of_ms: asOfMs, etag: answer.notModified ? copy.etag : answer.etag ?? null });
    return answer.asOfMs == null
      ? { state: "current", text }
      : { state: "stale", text, as_of_ms: asOfMs, why: "the server's store is not answering" };
  }
  if (!copy) throw new ReadFailed(`the upstream's ${name} could not be read (${why}), and the working copy holds none`);
  return { state: "stale", text: copy.text, as_of_ms: copy.as_of_ms, why: `the upstream did not answer (${why})` };
}

const isUrl = (upstream) => /^https?:\/\//.test(upstream);

// The working copy for one upstream: the last answer of each head, and
// its blobs by hash.
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
    head: (name) => ({
      load: () => (existsSync(join(dir, "heads", `${name}.json`)) ? JSON.parse(readFileSync(join(dir, "heads", `${name}.json`), "utf8")) : null),
      save: (copy) => write(join(dir, "heads", `${name}.json`), JSON.stringify(copy)),
    }),
    blob: (hash) => (existsSync(blobFile(hash)) ? readFileSync(blobFile(hash)) : null),
    keepBlob: (hash, bytes) => write(blobFile(hash), bytes),
  };
}

function limitAfter(ms) {
  return new Promise((_, reject) => setTimeout(() => reject(new Error(`no answer in ${ms} ms`)), ms).unref());
}

// A head, current or stale with its age. Its value is null when the
// upstream does not have it: an export has no `edits`. An export folder
// that does not exist is an upstream that does not answer.
export async function head(upstream, copy, name, { limitMs = LIMIT_MS } = {}) {
  const fetchHead = isUrl(upstream)
    ? async (etag) => {
        const resp = await fetch(new URL(name, upstream.endsWith("/") ? upstream : `${upstream}/`), {
          signal: AbortSignal.timeout(limitMs),
          headers: etag ? { "If-None-Match": etag } : {},
        });
        const stale = resp.headers.get("X-Store-Stale") === "true";
        const asOf = Number(resp.headers.get("X-Store-As-Of"));
        const asOfMs = stale ? (Number.isFinite(asOf) && asOf > 0 ? asOf : 0) : null;
        if (resp.status === 304) return { notModified: true, asOfMs };
        if (resp.status === 404) return { text: null, asOfMs };
        if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
        return { text: await resp.text(), asOfMs, etag: resp.headers.get("ETag") };
      }
    : async () => {
        if (!existsSync(upstream)) throw new Error(`${upstream} does not exist`);
        const file = join(upstream, name);
        return { text: existsSync(file) ? readFileSync(file, "utf8") : null, asOfMs: null };
      };
  const read = await readHead({ name, fetchHead, limit: limitAfter(limitMs), nowMs: Date.now, kept: copy.head(name) });
  return { ...read, value: read.text == null ? null : JSON.parse(read.text) };
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
