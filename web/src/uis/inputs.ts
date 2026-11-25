// A read answers a document, "none kept", or a failure, and never answers
// null for both (QAF-131). The rules are in docs/architecture/web.md, "A
// UI's inputs are declared as data". The file imports nothing, so its
// properties run under `node --test`.

export type InputDeclaration = {
  id: string;
  // Used in a label, such as "the detours".
  name: string;
  from: string;
  // Seconds. Null reads once, at boot.
  every_s: number | null;
  // Seconds. A read that has not answered by then has failed.
  limit_s: number;
  // "none" is for a live feed, because a page never shows an old prediction
  // as live.
  keeps: "last" | "none";
  // Seconds. The age is the upstream's as-of time when the reader names
  // one, else the time of the last answered read.
  old_after_s?: number | null;
  feeds: string[];
  // Its reader reads what those inputs adopted.
  after?: string[];
  // Names rows of the UI's failure table by their first cell, up to the
  // colon. A gate in `just test` matches each name to a row.
  fails_as: string[];
};

// The same `key` is the same document. `asOfMs` is set when the upstream
// answered from its working copy (`X-Store-As-Of`, the service worker's
// `Date`) or the document names its own time, and is absent for a current
// answer.
export type Answered<T> = { value: T; key: string; asOfMs?: number; why?: string };

// Any other error is a failure too.
export class ReadFailed extends Error {
  retryAfterMs: number | null;
  constructor(message: string, opts: { retryAfterMs?: number | null } = {}) {
    super(message);
    this.name = "ReadFailed";
    this.retryAfterMs = opts.retryAfterMs ?? null;
  }
}

// `read` takes `held`, what the page holds now, so it can answer the same
// key without fetching the whole document again. It answers null when the
// upstream keeps none.
export type Reader<T = unknown> = {
  read: (ctx: { signal: AbortSignal; held: T | null }) => Promise<Answered<T> | null>;
  adopt: (value: T | null) => void | Promise<void>;
};

export type InputState =
  | { kind: "reading"; input: string; name: string }
  | { kind: "answered"; input: string; name: string; atMs: number; asOfMs: number | null; why: string | null; old: boolean }
  | { kind: "none"; input: string; name: string; atMs: number }
  // `held` means the page still holds a document (keeps "last"). `refused`
  // means the upstream answered and a module refused the answer, so
  // `message` is the reason.
  | { kind: "failed"; input: string; name: string; sinceMs: number; message: string; held: boolean; asOfMs: number | null; old: boolean; refused: boolean };

// `states` is a `$state` object, so the shell redraws. `random` spreads the
// reads after a failure.
export type InputHost = {
  states: Record<string, InputState>;
  request: (...feeds: string[]) => Promise<void>;
  now: () => number;
  random: () => number;
  after: (ms: number, fn: () => void | Promise<void>) => () => void;
  isVisible: () => boolean;
  onVisible: (fn: () => void | Promise<void>) => void;
};

export type Inputs = {
  // Settles when the first reads are adopted and the graph has answered them.
  start: () => Promise<void>;
  read: (...ids: string[]) => Promise<void>;
  // A page that applies a new build stops its reads before it reloads,
  // because the old build's worker hands over only when it has no request
  // in flight.
  stop: () => void;
};

const RETRY_BOOT_S = 60;

const messageOf = (e: unknown) => (e instanceof Error ? e.message : String(e));

export function inputs(declared: InputDeclaration[], readers: Record<string, Reader<any>>, host: InputHost): Inputs {
  const ids = new Set(declared.map((d) => d.id));
  const earlier = new Set<string>();
  for (const d of declared) {
    if (!readers[d.id]) throw new Error(`input ${d.id} is declared and has no reader`);
    for (const a of d.after ?? []) if (!earlier.has(a)) throw new Error(`input ${d.id} reads after ${a}, which is not declared before it`);
    earlier.add(d.id);
  }
  for (const id of Object.keys(readers)) if (!ids.has(id)) throw new Error(`input ${id} has a reader and is not declared`);

  const held = new Map<string, { value: unknown; key: string; confirmedMs: number } | null>();
  const dueAt = new Map<string, number>();
  for (const d of declared) host.states[d.id] = { kind: "reading", input: d.id, name: d.name };

  const oldAt = (d: InputDeclaration, confirmedMs: number) => d.old_after_s != null && host.now() - confirmedMs > d.old_after_s * 1000;

  async function attempt(d: InputDeclaration): Promise<{ ok: Answered<unknown> | null } | { failed: unknown }> {
    const controller = new AbortController();
    let cancel = () => {};
    const limit = new Promise<never>((_, reject) => {
      cancel = host.after(d.limit_s * 1000, () => {
        controller.abort();
        reject(new ReadFailed(`no answer in ${d.limit_s} s`));
      });
    });
    try {
      const answer = await Promise.race([readers[d.id].read({ signal: controller.signal, held: held.get(d.id)?.value ?? null }), limit]);
      return { ok: answer };
    } catch (e) {
      return { failed: e };
    } finally {
      cancel();
    }
  }

  async function readOne(d: InputDeclaration, changed: Set<string>): Promise<void> {
    const startedMs = host.now();
    const outcome = await attempt(d);
    const before = host.states[d.id];
    const had = held.get(d.id) ?? null;
    let moved = false;

    // An adopt that throws, because a module refused the document, is a
    // failed read.
    const adopt = async (value: unknown): Promise<unknown> => {
      try {
        await readers[d.id].adopt(value);
        moved = true;
        return null;
      } catch (e) {
        return e ?? new Error("the page did not take it");
      }
    };

    let failure: unknown = "ok" in outcome ? null : outcome.failed;
    if ("ok" in outcome) {
      const answer = outcome.ok;
      if (answer) {
        const confirmedMs = answer.asOfMs ?? host.now();
        if (!had || had.key !== answer.key) failure = await adopt(answer.value);
        if (!failure) {
          held.set(d.id, { value: answer.value, key: answer.key, confirmedMs });
          host.states[d.id] = { kind: "answered", input: d.id, name: d.name, atMs: host.now(), asOfMs: answer.asOfMs ?? null, why: answer.why ?? null, old: oldAt(d, confirmedMs) };
        }
      } else {
        if (had) failure = await adopt(null);
        if (!failure) {
          held.set(d.id, null);
          host.states[d.id] = { kind: "none", input: d.id, name: d.name, atMs: host.now() };
        }
      }
      if (!failure) dueAt.set(d.id, d.every_s == null ? Infinity : startedMs + d.every_s * 1000);
    }
    if (failure) {
      const retryAfterMs = failure instanceof ReadFailed ? (failure.retryAfterMs ?? 0) : 0;
      // The random part keeps pages that failed together from reading again
      // at the same moment. A boot-only input that failed is read again
      // until it answers.
      const every = (d.every_s ?? RETRY_BOOT_S) * 1000;
      dueAt.set(d.id, startedMs + Math.max(every, retryAfterMs) + host.random() * every * 0.5);
      if (had && d.keeps === "none" && !(await adopt(null))) held.set(d.id, null);
      const kept = held.get(d.id) ?? null;
      const sinceMs = before.kind === "failed" ? before.sinceMs : startedMs;
      const asOfMs = kept ? kept.confirmedMs : null;
      host.states[d.id] = { kind: "failed", input: d.id, name: d.name, sinceMs, message: messageOf(failure), held: !!kept, asOfMs, old: kept ? oldAt(d, kept.confirmedMs) : false, refused: !(failure instanceof ReadFailed) };
    }
    // A change of the state's kind answers the graph too, so a node that
    // requires the input says why.
    if (moved || before.kind !== host.states[d.id].kind) for (const f of d.feeds) changed.add(f);
  }

  async function readAll(due: InputDeclaration[]): Promise<void> {
    const changed = new Set<string>();
    const reading = new Map<string, Promise<void>>();
    for (const d of due) {
      const first = (d.after ?? []).map((a) => reading.get(a)).filter((p): p is Promise<void> => !!p);
      reading.set(d.id, first.length ? Promise.all(first).then(() => readOne(d, changed)) : readOne(d, changed));
    }
    await Promise.all(reading.values());
    if (changed.size) await host.request(...changed);
  }

  let timer: (() => void) | null = null;
  let ticking = false;
  let stopped = false;

  async function tick(): Promise<void> {
    timer?.();
    timer = null;
    if (stopped || !host.isVisible() || ticking) return;
    ticking = true;
    try {
      const now = host.now();
      await readAll(declared.filter((d) => (dueAt.get(d.id) ?? 0) <= now));
    } finally {
      ticking = false;
      schedule();
    }
  }

  function schedule(): void {
    timer?.();
    timer = null;
    if (stopped) return;
    const next = Math.min(...declared.map((d) => dueAt.get(d.id) ?? Infinity));
    if (Number.isFinite(next)) timer = host.after(Math.max(0, next - host.now()), tick);
  }

  return {
    start: async () => {
      host.onVisible(tick);
      await readAll(declared);
      schedule();
    },
    read: async (...wanted: string[]) => {
      if (stopped) return;
      await readAll(declared.filter((d) => wanted.includes(d.id)));
      schedule();
    },
    stop: () => {
      stopped = true;
      timer?.();
      timer = null;
    },
  };
}

export function capital(name: string): string {
  const bare = name.replace(/^the /, "");
  return bare.charAt(0).toUpperCase() + bare.slice(1);
}

// The wording follows docs/architecture/hosts.md, "The rules every host
// keeps". Returns "" when the input is current, not read yet, or kept by
// none. `clock` names an instant on the agency's clock ("4:05 PM").
export function labelOf(state: InputState | undefined, clock: (ms: number) => string): string {
  if (!state) return "";
  const what = capital(state.name);
  if (state.kind === "answered") {
    if (state.asOfMs != null && state.old) return `${what} as of ${clock(state.asOfMs)}: not updated since.`;
    if (state.asOfMs != null) return `${what} as of ${clock(state.asOfMs)}: ${state.why ?? "the server is not answering"}.`;
    if (state.old) return `${what} as of ${clock(state.atMs)}: not updated since.`;
    return "";
  }
  if (state.kind === "failed") {
    const reason = state.message.replace(/\.$/, "");
    if (state.refused && state.held && state.asOfMs != null) return `${what} as of ${clock(state.asOfMs)}: ${reason}.`;
    if (state.refused) return `${what} could not be read: ${reason}.`;
    if (state.held && state.asOfMs != null) return `${what} as of ${clock(state.asOfMs)}: no answer since ${clock(state.sinceMs)}.`;
    return `${what} could not be read: no answer since ${clock(state.sinceMs)}.`;
  }
  return "";
}
