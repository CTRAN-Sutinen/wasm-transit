/// The page's outbox (D-189): the writes the server has not kept yet,
/// kept in the device's store (IndexedDB) so a reload keeps them, and
/// sent in order through `writeFetch` when the server answers. The core
/// and its rules are src/outbox.ts. Host I/O only: no module is called
/// here.
///
/// A host opens it at boot (`openOutbox`), queues each write the person
/// saves (`queueWrite`), and shows `outbox`: how many writes wait, the
/// ones the server refused with its reasons, and whether the server
/// answers. It sends what waits again on its own: when the device comes
/// back online, on a timer while anything waits, and when the host asks
/// (`sendWaiting`, at each poll).

import { get as idbGet, set as idbSet } from "idb-keyval";
import { Outbox, type Outcome, type Waiting, type Write } from "./outbox";
import { writeFetch } from "./signin.svelte";

export type { Outcome, Waiting, Write } from "./outbox";

/// What the page shows: the writes that wait, the writes the server
/// refused, and the connection as the last attempt found it.
export const outbox = $state({
  waiting: 0,
  held: [] as Waiting[],
  /// The last write could not reach the server.
  offline: false,
  /// The server asks for sign-in before it takes what waits.
  needsSignIn: false,
});

let box: Outbox | null = null;

/// A copy the device's store can keep: plain data, with a photo kept as
/// its bytes (a Blob). A write's arguments may come from the page's
/// state, which IndexedDB cannot clone as it is.
function plain(value: unknown): unknown {
  if (value instanceof Blob || value === null || typeof value !== "object") return value;
  if (Array.isArray(value)) return value.map(plain);
  return Object.fromEntries(Object.entries(value).filter(([, v]) => v !== undefined).map(([k, v]) => [k, plain(v)]));
}
let timer: ReturnType<typeof setInterval> | null = null;

/// How often the page tries again while writes wait.
const RETRY_MS = 15_000;

/// The answer's body: JSON when it is, else its text.
async function bodyOf(resp: Response): Promise<unknown> {
  const text = await resp.text();
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

/// Open the page's outbox (one per generated UI, `ui`), read back what
/// waited before a reload, and send it. `onsent` hears each write the
/// server kept, with its answer (the host reads its documents again).
export async function openOutbox(ui: string, onsent: (w: Waiting, body: unknown) => void = () => {}): Promise<void> {
  const key = `outbox/${ui}`;
  box = new Outbox({
    send: async (url, body, contentType) => {
      const resp = contentType
        ? await writeFetch(url, { method: "POST", headers: { "Content-Type": contentType }, body: body as Blob })
        : await writeFetch(url, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
      const retry = Number(resp.headers.get("Retry-After"));
      return { status: resp.status, body: await bodyOf(resp), retryAfter: Number.isFinite(retry) ? retry : undefined };
    },
    store: {
      load: () => idbGet(key).catch(() => undefined),
      save: (items) => idbSet(key, plain(items)),
    },
    now: () => Date.now(),
    newKey: () => `${ui}/${crypto.randomUUID()}`,
  });
  box.onchange = show;
  box.onsent = onsent;
  await box.restore();
  addEventListener("online", () => void sendWaiting());
  timer ??= setInterval(() => {
    if (box && box.waiting > 0 && Date.now() >= box.retryAtMs) void box.flush();
  }, RETRY_MS);
  void sendWaiting();
}

function show(): void {
  if (!box) return;
  outbox.waiting = box.waiting;
  outbox.held = box.items.filter((i) => i.state === "held");
  outbox.offline = box.offline;
  outbox.needsSignIn = box.needsSignIn;
}

/// A write the person saved: kept on the device, then sent after what
/// waits before it. The outcome says whether the server kept it, it
/// waits, or the server refused it (kept, with its reasons).
export async function queueWrite(write: Write): Promise<Outcome> {
  if (!box) throw new Error("the outbox is not open (openOutbox at boot)");
  return box.queue(write);
}

/// Send what waits now (a host's poll, the person's "Send now").
export async function sendWaiting(): Promise<void> {
  await box?.flush();
}

/// A held write sent again, at its place, with the log's seq the page
/// read since (`readSeq`).
export async function retryWrite(key: string, change: { readSeq?: number } = {}): Promise<Outcome> {
  if (!box) return { key, state: "waiting" };
  return box.retry(key, change);
}

/// The person drops a held write.
export async function discardWrite(key: string): Promise<void> {
  await box?.discard(key);
}

/// Why the server refused a held write, in words.
export function heldReason(w: Waiting): string {
  const body = (w.reasons ?? {}) as { refusals?: { problem?: { kind?: string } }[]; conflict?: string; error?: string };
  if (w.status === 409) return "Someone else changed this since you read it. Check it, then send it again or discard it.";
  if (w.status === 403) return "Your sign-in can't save this.";
  if (w.status === 422 && Array.isArray(body.refusals) && body.refusals.length) {
    const kinds = body.refusals.map((r) => (r.problem?.kind ?? "").replaceAll("_", " ")).filter(Boolean);
    return kinds.length ? `Not saved: ${[...new Set(kinds)].join("; ")}.` : "The server refused it.";
  }
  if (typeof body.error === "string") return body.error;
  if (typeof w.reasons === "string" && w.reasons.trim()) return w.reasons.trim();
  return `The server refused it (${w.status}).`;
}
