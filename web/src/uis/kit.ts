/// The generated UIs' shared host glue — what every UI's host layer
/// needs and none should rewrite. Host layer only: shells and
/// components never import this (it reaches the resolver).
///
/// A generated UI lives in `src/uis/<id>/` (docs/ARCHITECTURE.md,
/// "Generated UIs"): `ui.json` (what it answers), `main.ts` (mount,
/// then boot), `Shell.svelte` (placement only), `host.svelte.ts` (the
/// only file that calls the bus), and `<id>.html` at the web root.

import { analysisModule } from "../capabilities.svelte";
import type { Network } from "../types/Network";

/// Any assembled module (the resolver keeps the class private).
export type CapModule = NonNullable<Awaited<ReturnType<typeof analysisModule>>>;

const registered = new WeakMap<CapModule, WeakMap<Network, string>>();

/// A document's hash in one module — registered on first use, once
/// per module per document version. THE MODULE IS THE HASH AUTHORITY:
/// the host never mints a document's identity.
export function hashIn(module: CapModule, doc: Network): string {
  let docs = registered.get(module);
  if (!docs) registered.set(module, (docs = new WeakMap()));
  let hash = docs.get(doc);
  if (!hash) docs.set(doc, (hash = module.loadNetwork(doc)));
  return hash;
}

/// Record a document a module already registered (cap-author's
/// `apply` answers the next document under its hash) — the next
/// query skips the re-registration.
export function rememberHash(module: CapModule, doc: Network, hash: string): void {
  let docs = registered.get(module);
  if (!docs) registered.set(module, (docs = new WeakMap()));
  docs.set(doc, hash);
}

/// The local date, "YYYY-MM-DD" — hosts own the clock; dates cross
/// every boundary as strings.
export function today(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

// ---------------------------------------------------------------------
// TRANSIENT — the answer graph, hand-built ahead of the engine.
//
// docs/units/host-discipline.md designs the real composition layer:
// one engine that makes every op call, interpreting flow maps (data
// in the UI manifest, docs/units/wire-time-assembly.md) with
// generations and a hash-keyed journal. Until it lands, a host
// declares what it answers as nodes and this runner schedules them.
// It is shaped to be REPLACED by that engine, not grown into it:
//
// - a node is what a flow-map entry will be — the inputs it reads,
//   the frame that renders it, when that frame is on screen, and
//   the op calls — so a host's node list is the first draft of its
//   flow map;
// - `answer`'s key is the journal's (module bytes, input hashes,
//   arguments), so the cache it stands in for is the journal's;
// - neither knows anything about transit. If a node name, a field,
//   or a rule of the domain appears in this section, it leaked.
//
// Do not add features here; add them to the unit.
// ---------------------------------------------------------------------

/// One answer a host keeps: an op (or a short run of ops over the
/// same inputs) and the host state it writes.
export type AnswerNode = {
  /// The op(s) it answers ("analysis.service_groups"), or
  /// "document.<name>" for a document read before any op sees it.
  id: string;
  /// The component that renders the answer ("" for a document read).
  frame: string;
  /// The inputs (selection keys, document keys) and earlier nodes'
  /// ids it reads: a change to any of them re-answers it. A node may
  /// only name nodes declared before it.
  needs: string[];
  /// False while its frame is off screen: the node is not answered,
  /// `hide` clears what it wrote, and its dependents count as
  /// changed. Name the visibility input in `needs` so showing the
  /// frame answers it.
  visible?: () => boolean;
  hide?: () => void;
  /// Answer, write the host state, and `touch` any input it repaired
  /// (a selection the new answer no longer holds) — later nodes that
  /// read that input re-answer in the same pass.
  run: (touch: (input: string) => void) => Promise<void>;
};

const yieldToPaint = () => new Promise<void>((r) => setTimeout(r, 0));

/// The runner: `request(...inputs)` marks inputs changed and answers
/// every node downstream of them, in declared order, yielding between
/// nodes so each frame paints as its answer lands. A request made
/// while a pass runs supersedes it: the pass stops at the next node
/// and one merged pass starts from the top (nodes already answered
/// hit `answer`'s cache). The returned promise settles when no
/// request is left.
export function answerGraph(nodes: AnswerNode[], onerror: (e: unknown) => void) {
  const declared = new Set<string>();
  const ids = new Set(nodes.map((n) => n.id));
  for (const node of nodes) {
    for (const need of node.needs) {
      if (ids.has(need) && !declared.has(need)) throw new Error(`answer node ${node.id} reads ${need} before it is declared`);
    }
    declared.add(node.id);
  }

  let pending = new Set<string>();
  let draining: Promise<void> | null = null;

  async function pass(changed: Set<string>): Promise<void> {
    for (const node of nodes) {
      if (!node.needs.some((n) => changed.has(n))) continue;
      if (node.visible && !node.visible()) {
        node.hide?.();
        changed.add(node.id);
        continue;
      }
      await yieldToPaint();
      if (pending.size) {
        for (const c of changed) pending.add(c);
        return;
      }
      try {
        await node.run((input) => changed.add(input));
      } catch (e) {
        onerror(e);
      }
      changed.add(node.id);
    }
  }

  return function request(...inputs: string[]): Promise<void> {
    for (const input of inputs) pending.add(input);
    draining ??= (async () => {
      try {
        while (pending.size) {
          const changed = pending;
          pending = new Set();
          await pass(changed);
        }
      } finally {
        draining = null;
      }
    })();
    return draining;
  };
}

const answers = new Map<string, unknown>();
const ANSWERS_KEPT = 256;

/// An op's answer, cached by what determines it: the module's bytes,
/// the op, and its arguments. `args` are content hashes and plain
/// values only — never a document (a document crosses by its hash).
/// Ops are pure functions of those (modules carry no clock and no
/// randomness), so a hit is the same answer by construction. Errors
/// are not cached.
export function answer<T>(module: CapModule, op: string, args: unknown[], run: () => T): T {
  const key = JSON.stringify([module.cap.hash, op, args]);
  if (answers.has(key)) {
    const hit = answers.get(key) as T;
    answers.delete(key);
    answers.set(key, hit);
    return hit;
  }
  const value = run();
  answers.set(key, value);
  if (answers.size > ANSWERS_KEPT) answers.delete(answers.keys().next().value as string);
  return value;
}
