import { idbGet, idbSet } from "./idb";
import { Outbox, type Outcome, type Waiting, type Write } from "./outbox";
import { writeFetch } from "./signin.svelte";

export type { Outcome, Waiting, Write } from "./outbox";

export const outbox = $state({
  waiting: 0,
  held: [] as Waiting[],
  offline: false,
  needsSignIn: false,
  // Writes that the server kept and that leave a broken reference (D-233).
  warned: [] as Warned[],
});

export type BrokenReference = { log: string; kind: string; id: string; field: string; target: string; names: string; rule?: string };

export type Warned = { key: string; label: string; references: BrokenReference[] };

let box: Outbox | null = null;

// A write's arguments may come from the page's state, which IndexedDB
// cannot clone. A photo stays a Blob.
function plain(value: unknown): unknown {
  if (value instanceof Blob || value === null || typeof value !== "object") return value;
  if (Array.isArray(value)) return value.map(plain);
  return Object.fromEntries(Object.entries(value).filter(([, v]) => v !== undefined).map(([k, v]) => [k, plain(v)]));
}
let timer: ReturnType<typeof setInterval> | null = null;

const RETRY_MS = 15_000;

async function bodyOf(resp: Response): Promise<unknown> {
  const text = await resp.text();
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

// One outbox for each generated UI. `onsent` lets the host read its
// documents again.
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
  box.onsent = (w, body) => {
    const references = (body as { references?: BrokenReference[] } | null)?.references;
    if (Array.isArray(references) && references.length) outbox.warned = [...outbox.warned, { key: w.key, label: w.label, references }];
    onsent(w, body);
  };
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

export async function queueWrite(write: Write): Promise<Outcome> {
  if (!box) throw new Error("the outbox is not open (openOutbox at boot)");
  return box.queue(write);
}

export async function sendWaiting(): Promise<void> {
  await box?.flush();
}

export async function retryWrite(key: string, change: { readSeq?: number; readSeqs?: Record<string, number> } = {}): Promise<Outcome> {
  if (!box) return { key, state: "waiting" };
  return box.retry(key, change);
}

export async function discardWrite(key: string): Promise<void> {
  await box?.discard(key);
}

export function heldReason(w: Waiting): string {
  const body = (w.reasons ?? {}) as { refusals?: { problem?: { kind?: string } }[]; references?: BrokenReference[]; conflict?: string; error?: string | { message?: string } };
  if (w.status === 409) return "Someone else changed this since you read it. Check it, then send it again or discard it.";
  if (w.status === 403) return "Your sign-in can't save this.";
  if (w.status === 422 && Array.isArray(body.references) && body.references.length) return body.references.map(referenceWarning).join(" ");
  if (w.status === 422 && Array.isArray(body.refusals) && body.refusals.length) {
    const kinds = body.refusals.map((r) => (r.problem?.kind ?? "").replaceAll("_", " ")).filter(Boolean);
    return kinds.length ? `Not saved: ${[...new Set(kinds)].join("; ")}.` : "The server refused it.";
  }
  if (typeof body.error === "string") return body.error;
  if (typeof body.error?.message === "string") return body.error.message;
  if (typeof w.reasons === "string" && w.reasons.trim()) return w.reasons.trim();
  return `The server refused it (${w.status}).`;
}

export function warnedWrites(): { key: string; label: string; warning: string }[] {
  return outbox.warned.map((w) => ({ key: w.key, label: w.label, warning: w.references.map(referenceWarning).join(" ") }));
}

export function dismissWarning(key: string): void {
  outbox.warned = outbox.warned.filter((w) => w.key !== key);
}

// A rule's reference ends with the module's words for the target's state
// (`rule`).
export function referenceWarning(r: BrokenReference): string {
  const words = (kind: string) => kind.replaceAll("_", " ");
  const what = words(r.kind);
  return `${what.charAt(0).toUpperCase()}${what.slice(1)} ${r.id} names ${words(r.target)} ${r.names}, which ${r.rule ?? "isn't on record"}.`;
}
