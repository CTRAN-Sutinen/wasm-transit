/// The device's document store: versioned local data, immutable
/// history, persisted to IndexedDB. Host I/O — no module is called
/// here (the engine, src/engine.ts, is the one op caller).
///
/// A migration never destroys: the migrated document is a NEW record
/// (its own content hash) whose `produced_by` names the record it came
/// from. `current` is the only mutable thing — a pointer to the record
/// in force; earlier records stay addressable. The whole store
/// persists to IndexedDB as plain data (the project's persistence
/// rule: state survives reloads; proxies never cross).

import { get as idbGet, set as idbSet } from "idb-keyval";

export type DocRecord = {
  hash: string;
  version: number;
  /// Canonical JSON of the document.
  bytes: string;
  /// How this version came to exist. Absent = authored/seeded.
  produced_by?: { op: "migrate"; from: string; by: string };
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

/// Hash a document's canonical bytes (the string IS the bytes here —
/// documents cross the ABI as JSON).
async function hashDocument(bytes: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(bytes));
  const hex = Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
  return "sha256:" + hex;
}

/// Store a document version; idempotent by content hash. Seeding with
/// the same bytes twice lands on the same record.
export async function putDocument(kind: string, bytes: string, version: number): Promise<DocRecord> {
  await restoreDocuments();
  const hash = await hashDocument(bytes);
  const record: DocRecord = { hash, version, bytes };
  const entry = (documents.kinds[kind] ??= { records: {}, current: null });
  entry.records[hash] = record;
  entry.current = hash;
  await persistDocuments();
  return record;
}

export function currentDocument(kind: string): DocRecord | null {
  const entry = documents.kinds[kind];
  if (!entry?.current) return null;
  return entry.records[entry.current] ?? null;
}
