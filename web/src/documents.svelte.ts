// A record is keyed by the hash of its bytes. A document's identity on the
// bus is the module's answer (its load), not this key.
//
// A migration writes a new record whose `produced_by` names its source,
// and `current` is the only mutable pointer. A kind that is written on
// every use (a rider's recent places) passes `replace` and keeps only the
// records that a migration came from.

import { get as idbGet, set as idbSet } from "idb-keyval";

export type DocRecord = {
  hash: string;
  version: number;
  bytes: string;
  // Absent for an authored or seeded record.
  produced_by?: { op: "migrate"; from: string; by: string } | { op: "moved"; from: string };
};

const DOC_KEY = "caps-documents";

type DocKinds = Record<string, { records: Record<string, DocRecord>; current: string | null }>;

export const documents = $state({
  kinds: {} as DocKinds,
  restored: false,
});

async function persistDocuments(): Promise<void> {
  await idbSet(DOC_KEY, JSON.parse(JSON.stringify($state.snapshot(documentData()))));
}

function documentData(): { kinds: DocKinds } {
  return { kinds: documents.kinds };
}

export async function restoreDocuments(): Promise<void> {
  if (documents.restored) return;
  const stored = await idbGet<{ kinds: DocKinds }>(DOC_KEY);
  if (stored?.kinds) documents.kinds = stored.kinds;
  documents.restored = true;
}

async function hashDocument(bytes: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(bytes));
  const hex = Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
  return "sha256:" + hex;
}

// Idempotent by the bytes' hash.
export async function putDocument(
  kind: string,
  bytes: string,
  version: number,
  opts: { produced_by?: DocRecord["produced_by"]; replace?: boolean } = {},
): Promise<DocRecord> {
  await restoreDocuments();
  const hash = await hashDocument(bytes);
  const record: DocRecord = { hash, version, bytes, ...(opts.produced_by ? { produced_by: opts.produced_by } : {}) };
  const entry = (documents.kinds[kind] ??= { records: {}, current: null });
  if (opts.replace) {
    const sources = new Set(Object.values(entry.records).flatMap((r) => (r.produced_by?.op === "migrate" ? [r.produced_by.from] : [])));
    for (const key of Object.keys(entry.records)) if (!sources.has(key)) delete entry.records[key];
  }
  entry.records[hash] = record;
  entry.current = hash;
  await persistDocuments();
  return record;
}

// Keeps what a document was before a migration wrote over it elsewhere.
export async function keepRecord(kind: string, bytes: string, version: number): Promise<DocRecord> {
  await restoreDocuments();
  const hash = await hashDocument(bytes);
  const record: DocRecord = { hash, version, bytes };
  const entry = (documents.kinds[kind] ??= { records: {}, current: null });
  entry.records[hash] = record;
  await persistDocuments();
  return record;
}

export function currentDocument(kind: string): DocRecord | null {
  const entry = documents.kinds[kind];
  if (!entry?.current) return null;
  return entry.records[entry.current] ?? null;
}
