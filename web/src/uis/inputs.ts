/// The inputs. A UI's inputs are declarations, and this runner reads
/// them, as the answer graph (answers.ts) runs the answers and the
/// outbox (src/outbox.ts) runs the writes. An input is a document or a
/// feed from outside the page: the served network, the edit document, a
/// log's document, a live feed.
///
/// - A UI declares each input in its `ui.json` (`inputs`): where it
///   comes from, how often it is read, its time limit, what the page
///   keeps when a read fails, and the age past which it is old. These
///   are data, so the gates and the docs site read them as data.
/// - The host gives each declared input a reader: `read` answers it,
///   and `adopt` writes it into the host's state.
/// - The runner reads every input with one poll while the page is
///   visible. It stops a read at its time limit. A read answers a
///   document, "none kept", or a failure. It never answers null for
///   both "none kept" and "failed" (QAF-131).
/// - The input's state (`states[id]`) names the input, so a frame that
///   reads it can say why it shows what it shows.
/// - A change to what the page holds changes the input's graph inputs
///   (`feeds`), in one request for each poll.
///
/// The laws are in inputs.test.mjs. This file imports nothing, so they
/// run under `node --test`. It knows nothing about transit.

/// One input, as a UI's `ui.json` declares it.
export type InputDeclaration = {
  /// The input's id: the host's reader has the same id.
  id: string;
  /// What the page calls it, in a label ("the detours").
  name: string;
  /// Where it comes from, in words ("the server's edit document,
  /// GET edits").
  from: string;
  /// How often it is read, in seconds. null: read once, at boot.
  every_s: number | null;
  /// The time limit of one read, in seconds. A read that has not
  /// answered by then has failed.
  limit_s: number;
  /// What the page keeps when a read fails: "last", its last document,
  /// labelled with its age; "none", nothing (a live feed: a page never
  /// shows an old prediction as live).
  keeps: "last" | "none";
  /// The age, in seconds, past which the document is old, or null for
  /// none. The age is the upstream's as-of time when the reader names
  /// one, else the time of the last answered read.
  old_after_s?: number | null;
  /// The answer graph's inputs it changes.
  feeds: string[];
  /// The inputs read before it in the same poll: its reader reads what
  /// they adopted.
  after?: string[];
  /// The rows of the UI's failure table (docs/architecture/
  /// generated-uis/<id>.md) that say what the page does while this input's
  /// reads fail, by each row's name (its first cell, to the colon). A
  /// gate in `just test` matches each name to a row.
  fails_as: string[];
};

/// A read's document. `key` names its content (a hash, or the text):
/// the same key is the same document, and nothing changes.
/// `asOfMs` is when the upstream last confirmed it, when the upstream
/// answered from its working copy (the server's `X-Store-As-Of`, the
/// service worker's copy's `Date`) or the document names its own time
/// (a live feed's timestamp). Absent for an answer that is current.
/// `why` says why it is a working copy, in the upstream's words ("the
/// server's store is not answering").
export type Answered<T> = { value: T; key: string; asOfMs?: number; why?: string };

/// A read that failed, with the time the upstream asks the page to
/// wait (its `Retry-After`). Any other error is a failure too.
export class ReadFailed extends Error {
  retryAfterMs: number | null;
  constructor(message: string, opts: { retryAfterMs?: number | null } = {}) {
    super(message);
    this.name = "ReadFailed";
    this.retryAfterMs = opts.retryAfterMs ?? null;
  }
}

/// The host's reader of one input. `read` answers the document, null
/// when the upstream keeps none, or throws when the read failed. It
/// reads `held`, what the page holds now, to answer the same key
/// without fetching the whole document again. `adopt` writes the
/// document (null: none) into the host's state.
export type Reader<T = unknown> = {
  read: (ctx: { signal: AbortSignal; held: T | null }) => Promise<Answered<T> | null>;
  adopt: (value: T | null) => void | Promise<void>;
};

/// What the page knows of one input.
export type InputState =
  /// Not read yet.
  | { kind: "reading"; input: string; name: string }
  /// The last read answered a document. `asOfMs`: see `Answered`.
  | { kind: "answered"; input: string; name: string; atMs: number; asOfMs: number | null; why: string | null; old: boolean }
  /// The last read answered that the upstream keeps none.
  | { kind: "none"; input: string; name: string; atMs: number }
  /// The reads fail, since `sinceMs`. `held`: the page still holds a
  /// document (keeps: "last"), confirmed at `asOfMs`. `refused`: the
  /// upstream answered and the page refused the answer (a module's
  /// refusal, not a ReadFailed), so `message` is the reason.
  | { kind: "failed"; input: string; name: string; sinceMs: number; message: string; held: boolean; asOfMs: number | null; old: boolean; refused: boolean };

/// What the runner needs from its host: the record of input states (a
/// `$state` object, so the shell redraws), the answer graph's request,
/// the clock, a timer, the page's visibility, and a random number (the
/// spread of reads after a failure). kit.ts gives the browser's.
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
  /// The first read of every input, then the poll. It settles when the
  /// first reads are adopted and the graph has answered them.
  start: () => Promise<void>;
  /// Read these inputs now (a write moved their head), and answer what
  /// changed.
  read: (...ids: string[]) => Promise<void>;
  /// Read nothing more. A page that applies a new build stops its reads
  /// before it reloads: the old build's worker hands over only when it
  /// has no request in flight.
  stop: () => void;
};

/// How often a boot-only input that failed is read again, in seconds.
const RETRY_BOOT_S = 60;

const messageOf = (e: unknown) => (e instanceof Error ? e.message : String(e));

/// The runner.
export function inputs(declared: InputDeclaration[], readers: Record<string, Reader<any>>, host: InputHost): Inputs {
  const ids = new Set(declared.map((d) => d.id));
  const earlier = new Set<string>();
  for (const d of declared) {
    if (!readers[d.id]) throw new Error(`input ${d.id} is declared and has no reader`);
    for (const a of d.after ?? []) if (!earlier.has(a)) throw new Error(`input ${d.id} reads after ${a}, which is not declared before it`);
    earlier.add(d.id);
  }
  for (const id of Object.keys(readers)) if (!ids.has(id)) throw new Error(`input ${id} has a reader and is not declared`);

  // What the page holds of each input, the key of it, when its upstream
  // last confirmed it, and when the input is next due.
  const held = new Map<string, { value: unknown; key: string; confirmedMs: number } | null>();
  const dueAt = new Map<string, number>();
  for (const d of declared) host.states[d.id] = { kind: "reading", input: d.id, name: d.name };

  const oldAt = (d: InputDeclaration, confirmedMs: number) => d.old_after_s != null && host.now() - confirmedMs > d.old_after_s * 1000;

  /// One read with its time limit. It answers the read's outcome; it
  /// never throws.
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

  /// Read one input, adopt what changed, and name the graph inputs to
  /// answer again.
  async function readOne(d: InputDeclaration, changed: Set<string>): Promise<void> {
    const startedMs = host.now();
    const outcome = await attempt(d);
    const before = host.states[d.id];
    const had = held.get(d.id) ?? null;
    let moved = false;

    /// Adopt a value. An adopt that throws (a module that refused the
    /// document) is a failed read: it answers the error, else null.
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
      // The next read waits its interval (or the upstream's
      // Retry-After) and a random part of the interval, so pages that
      // failed together do not all read again at the same moment. A
      // boot-only input that failed is read again until it answers.
      const every = (d.every_s ?? RETRY_BOOT_S) * 1000;
      dueAt.set(d.id, startedMs + Math.max(every, retryAfterMs) + host.random() * every * 0.5);
      if (had && d.keeps === "none" && !(await adopt(null))) held.set(d.id, null);
      const kept = held.get(d.id) ?? null;
      const sinceMs = before.kind === "failed" ? before.sinceMs : startedMs;
      const asOfMs = kept ? kept.confirmedMs : null;
      host.states[d.id] = { kind: "failed", input: d.id, name: d.name, sinceMs, message: messageOf(failure), held: !!kept, asOfMs, old: kept ? oldAt(d, kept.confirmedMs) : false, refused: !(failure instanceof ReadFailed) };
    }
    // The graph answers again when what the page holds changed, or the
    // state's kind did: a node that requires the input says why.
    if (moved || before.kind !== host.states[d.id].kind) for (const f of d.feeds) changed.add(f);
  }

  /// Read the given inputs, each after the ones it names, and answer
  /// the graph once.
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

  /// One poll: read the inputs that are due, then wait for the next.
  async function tick(): Promise<void> {
    timer?.();
    timer = null;
    if (stopped || !host.isVisible() || ticking) return;
    ticking = true;
    try {
      const now = host.now();
      await readAll(declared.filter((d) => (dueAt.get(d.id) ?? 0) <= now));
    } finally {
      // The poll goes on whatever the graph's answer did.
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

/// "the detours" → "Detours".
export function capital(name: string): string {
  const bare = name.replace(/^the /, "");
  return bare.charAt(0).toUpperCase() + bare.slice(1);
}

/// What the page says of an input, in the server's words
/// (docs/architecture/hosts.md, "The rules every host keeps"): its age
/// when the answer is a working copy or old, and since when its reads
/// fail, or the reason the page refused what it read. "" when it is
/// current, not read yet, or kept by none. `clock`
/// names an instant on the agency's clock ("4:05 PM").
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
