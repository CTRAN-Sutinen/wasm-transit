/// A head the server serves, as a read of an input answers it: the
/// rules every host keeps (docs/architecture/hosts.md), on the
/// browser's side. The kit's `readHead` fetches; this file reads the
/// answer. It imports only inputs.ts, so the host contract suite
/// (host-contract.test.mjs) runs it under `node --test`.

import { ReadFailed, type Answered } from "./inputs.ts";

/// What a read sees of an HTTP answer.
export type Seen = { status: number; header: (name: string) => string | null; text: string };

/// The upstream's Retry-After, in ms (seconds or an HTTP date), else
/// null.
export function retryAfterMs(value: string | null, nowMs: number): number | null {
  if (!value) return null;
  const secs = Number(value);
  if (Number.isFinite(secs)) return Math.max(0, secs * 1000);
  const at = Date.parse(value);
  return Number.isFinite(at) ? Math.max(0, at - nowMs) : null;
}

/// The answer of a head: its text, keyed by itself, with the server's
/// age when it answers from its cache (`X-Store-Stale`,
/// `X-Store-As-Of`; D-119): an old answer says its age, and the age is
/// the server's, never the time of this read. Null for an `absent`
/// status: the server keeps none. Throws ReadFailed for any other
/// status but 2xx, with the server's Retry-After.
export function headAnswer(seen: Seen, absent: number[], nowMs: number): Answered<string> | null {
  if (absent.includes(seen.status)) return null;
  if (seen.status < 200 || seen.status > 299) throw new ReadFailed(`HTTP ${seen.status}`, { retryAfterMs: retryAfterMs(seen.header("Retry-After"), nowMs) });
  if (seen.header("X-Store-Stale") !== "true") return { value: seen.text, key: seen.text };
  const asOf = Number(seen.header("X-Store-As-Of"));
  return { value: seen.text, key: seen.text, asOfMs: Number.isFinite(asOf) && asOf > 0 ? asOf : 0, why: "the server's store is not answering" };
}
