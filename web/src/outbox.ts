// The rules are in docs/architecture/updates-offline.md, "The outbox".
// This is the pure core. It never reads a write's ops.
// A log's write sends `read_seqs`, the other logs' seqs that the page read.
// A write that waited also carries `claimed_at_ms`.

// A route's body is JSON, with the key added when `keyed`, or a file sent
// as it is (`contentType`).
export type Write =
  | { log: string; ops: { cap: string; op: string; args: unknown }[]; readSeq?: number; readSeqs?: Record<string, number>; label: string; url?: undefined; body?: undefined }
  | { url: string; body: unknown; label: string; keyed?: boolean; contentType?: string; log?: undefined; ops?: undefined; readSeq?: undefined; readSeqs?: undefined };

export type Waiting = {
  key: string;
  label: string;
  // Unix ms on the device's clock.
  madeAtMs: number;
  attempts: number;
  // `held` means the server refused it and the person decides.
  state: "waiting" | "held";
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

export type Answer = { status: number; body: unknown; retryAfter?: number };

export type Outcome = { key: string } & ({ state: "sent"; body: unknown } | { state: "waiting" } | { state: "held"; status: number; reasons: unknown });

export type OutboxDeps = {
  // Throws when the server cannot be reached.
  send: (url: string, body: unknown, contentType?: string) => Promise<Answer>;
  store: { load: () => Promise<unknown>; save: (items: Waiting[]) => Promise<void> };
  now: () => number;
  newKey: () => string;
};

export class Outbox {
  items: Waiting[] = [];
  offline = false;
  needsSignIn = false;
  // Unix ms, or 0 when the server did not ask to wait.
  retryAtMs = 0;
  onchange: () => void = () => {};
  onsent: (w: Waiting, body: unknown) => void = () => {};

  #deps: OutboxDeps;
  #running: Promise<void> = Promise.resolve();
  #outcomes = new Map<string, Outcome>();

  constructor(deps: OutboxDeps) {
    this.#deps = deps;
  }

  async restore(): Promise<void> {
    const kept = await this.#deps.store.load();
    if (Array.isArray(kept)) this.items = kept as Waiting[];
    this.onchange();
  }

  async queue(write: Write): Promise<Outcome> {
    const item: Waiting = { key: this.#deps.newKey(), label: write.label, madeAtMs: this.#deps.now(), attempts: 0, state: "waiting" };
    if (write.log !== undefined) Object.assign(item, { log: write.log, ops: write.ops, readSeq: write.readSeq, readSeqs: write.readSeqs });
    else Object.assign(item, { url: write.url, body: write.body, keyed: write.keyed, contentType: write.contentType });
    this.items = [...this.items, item];
    await this.#save();
    await this.flush();
    return this.#outcomes.get(item.key) ?? { key: item.key, state: "waiting" };
  }

  flush(): Promise<void> {
    this.#running = this.#running.then(() => this.#run());
    return this.#running;
  }

  // Keeps the key and the write's place in the order.
  async retry(key: string, change: { readSeq?: number; readSeqs?: Record<string, number> } = {}): Promise<Outcome> {
    this.items = this.items.map((i) => (i.key === key ? { ...i, ...change, state: "waiting", status: undefined, reasons: undefined } : i));
    await this.#save();
    await this.flush();
    return this.#outcomes.get(key) ?? { key, state: "waiting" };
  }

  async discard(key: string): Promise<void> {
    this.items = this.items.filter((i) => i.key !== key);
    await this.#save();
  }

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
      // The writes stay on the page, and a reload would lose them.
      console.error("the outbox could not be kept on this device:", e);
    }
    this.onchange();
  }
}
