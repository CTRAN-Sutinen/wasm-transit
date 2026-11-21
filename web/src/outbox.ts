/// The outbox (D-189): the writes a page made that the server has not
/// kept yet, in the order the person made them. The server stays the
/// authority on writes. A write that cannot reach it waits on the
/// device, and is sent through the same receive path when the server
/// answers again.
///
/// - Every write has an idempotency key, made when the person saves.
///   Every attempt sends the same key, so a write whose answer was lost
///   lands once (the log engine answers a key that landed).
/// - A log's write is its intent: `{key, ops, read_seq, read_seqs}` to
///   `logs/<log>`. The server runs the ops over the head it holds.
///   `read_seqs` are the other logs' seqs the page read (the manifest's
///   log_seqs): a reference to a record missing from what the page read
///   is refused (D-233). A
///   write that waited also carries the time the person made it, as a
///   claim beside the server's stamp (`claimed_at_ms`, D-222).
/// - A network failure or a 5xx (the store not answering, D-119) keeps
///   the write waiting, and the writes after it wait behind it. A 401
///   holds the outbox until the person signs in. A refusal (400, 403,
///   409, 422) keeps the write, held with the server's reasons, and the
///   writes after it go on. Only the person drops a held write, or
///   sends it again.
///
/// This is the pure core: the page gives it how to send and where to
/// keep the list (src/outbox.svelte.ts). It never reads a write's ops.

/// A write as the page makes it: a log's intent, or a body for a route.
/// A route's body is JSON, with the key added when `keyed` (a route
/// that takes one), or a file sent as it is (`contentType`).
export type Write =
  | { log: string; ops: { cap: string; op: string; args: unknown }[]; readSeq?: number; readSeqs?: Record<string, number>; label: string; url?: undefined; body?: undefined }
  | { url: string; body: unknown; label: string; keyed?: boolean; contentType?: string; log?: undefined; ops?: undefined; readSeq?: undefined; readSeqs?: undefined };

/// One write that waits, or that the server refused.
export type Waiting = {
  key: string;
  label: string;
  /// The device's clock when the person made it (Unix ms).
  madeAtMs: number;
  /// How many times it was sent.
  attempts: number;
  /// waiting: not sent yet, or the server did not answer. held: the
  /// server refused it; the person decides.
  state: "waiting" | "held";
  /// The refusal's status and the server's reasons, when held.
  status?: number;
  reasons?: unknown;
  log?: string;
  ops?: { cap: string; op: string; args: unknown }[];
  readSeq?: number;
  readSeqs?: Record<string, number>;
  url?: string;
  body?: unknown;
  keyed?: boolean;
  contentType?: string;
};

/// What the server answered: its status and body (JSON when it is).
export type Answer = { status: number; body: unknown; retryAfter?: number };

/// How a queued write ended for now, by its key.
export type Outcome = { key: string } & ({ state: "sent"; body: unknown } | { state: "waiting" } | { state: "held"; status: number; reasons: unknown });

export type OutboxDeps = {
  /// One attempt: JSON, or a file sent as it is when `contentType` is
  /// given. Throws when the server cannot be reached.
  send: (url: string, body: unknown, contentType?: string) => Promise<Answer>;
  store: { load: () => Promise<unknown>; save: (items: Waiting[]) => Promise<void> };
  now: () => number;
  newKey: () => string;
};

export class Outbox {
  items: Waiting[] = [];
  /// The last attempt could not reach the server.
  offline = false;
  /// The server asks for sign-in: the outbox holds until the person
  /// signs in.
  needsSignIn = false;
  /// When the server asked to be tried again (Unix ms), else 0.
  retryAtMs = 0;
  /// Called after every change of the list or the flags.
  onchange: () => void = () => {};
  /// Called with each write the server kept, and its answer.
  onsent: (w: Waiting, body: unknown) => void = () => {};

  #deps: OutboxDeps;
  #running: Promise<void> = Promise.resolve();
  #outcomes = new Map<string, Outcome>();

  constructor(deps: OutboxDeps) {
    this.#deps = deps;
  }

  /// The list kept on the device, read back in order.
  async restore(): Promise<void> {
    const kept = await this.#deps.store.load();
    if (Array.isArray(kept)) this.items = kept as Waiting[];
    this.onchange();
  }

  /// A write the person made: kept on the device first, then sent with
  /// everything before it.
  async queue(write: Write): Promise<Outcome> {
    const item: Waiting = { key: this.#deps.newKey(), label: write.label, madeAtMs: this.#deps.now(), attempts: 0, state: "waiting" };
    if (write.log !== undefined) Object.assign(item, { log: write.log, ops: write.ops, readSeq: write.readSeq, readSeqs: write.readSeqs });
    else Object.assign(item, { url: write.url, body: write.body, keyed: write.keyed, contentType: write.contentType });
    this.items = [...this.items, item];
    await this.#save();
    await this.flush();
    return this.#outcomes.get(item.key) ?? { key: item.key, state: "waiting" };
  }

  /// Send what waits, in order. One run at a time.
  flush(): Promise<void> {
    this.#running = this.#running.then(() => this.#run());
    return this.#running;
  }

  /// A held write sent again (after the person read the head again:
  /// `readSeq`, and the other logs' in `readSeqs`), with its key, at its
  /// place in the order.
  async retry(key: string, change: { readSeq?: number; readSeqs?: Record<string, number> } = {}): Promise<Outcome> {
    this.items = this.items.map((i) => (i.key === key ? { ...i, ...change, state: "waiting", status: undefined, reasons: undefined } : i));
    await this.#save();
    await this.flush();
    return this.#outcomes.get(key) ?? { key, state: "waiting" };
  }

  /// The person drops a held write.
  async discard(key: string): Promise<void> {
    this.items = this.items.filter((i) => i.key !== key);
    await this.#save();
  }

  /// How many writes wait to be sent (not the held ones).
  get waiting(): number {
    return this.items.filter((i) => i.state === "waiting").length;
  }

  async #run(): Promise<void> {
    for (const item of [...this.items]) {
      if (item.state !== "waiting" || !this.items.some((i) => i.key === item.key)) continue;
      const [url, body] = this.#request(item);
      item.attempts += 1;
      let answer: Answer;
      try {
        answer = await this.#deps.send(url, body, item.contentType);
      } catch {
        this.offline = true;
        this.#outcomes.set(item.key, { key: item.key, state: "waiting" });
        return this.#save();
      }
      this.offline = false;
      if (answer.status === 401) {
        this.needsSignIn = true;
        this.#outcomes.set(item.key, { key: item.key, state: "waiting" });
        return this.#save();
      }
      this.needsSignIn = false;
      if (answer.status >= 500) {
        this.retryAtMs = this.#deps.now() + (answer.retryAfter ?? 0) * 1000;
        this.#outcomes.set(item.key, { key: item.key, state: "waiting" });
        return this.#save();
      }
      this.retryAtMs = 0;
      if (answer.status >= 200 && answer.status < 300) {
        this.items = this.items.filter((i) => i.key !== item.key);
        this.#outcomes.set(item.key, { key: item.key, state: "sent", body: answer.body });
        this.onsent(item, answer.body);
      } else {
        this.items = this.items.map((i) => (i.key === item.key ? { ...i, state: "held", status: answer.status, reasons: answer.body } : i));
        this.#outcomes.set(item.key, { key: item.key, state: "held", status: answer.status, reasons: answer.body });
      }
      await this.#save();
    }
  }

  /// The request of one attempt: a log's intent with its key (and the
  /// time it was made, once it waited), or a route's body as given.
  #request(item: Waiting): [string, unknown] {
    if (item.log === undefined) return [item.url!, item.keyed ? { ...(item.body as object), key: item.key } : item.body];
    const body: Record<string, unknown> = { key: item.key, ops: item.ops };
    if (item.readSeq !== undefined) body.read_seq = item.readSeq;
    if (item.readSeqs !== undefined) body.read_seqs = item.readSeqs;
    if (item.attempts > 0) body.claimed_at_ms = item.madeAtMs;
    return [`logs/${item.log}`, body];
  }

  async #save(): Promise<void> {
    try {
      await this.#deps.store.save(this.items);
    } catch (e) {
      // The device's store refused the list: the writes stay on the
      // page, and the console says why. A reload would lose them.
      console.error("the outbox could not be kept on this device:", e);
    }
    this.onchange();
  }
}
