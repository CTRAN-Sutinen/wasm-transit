/// The generated UIs' shared host glue — what every UI's host layer
/// needs and none should rewrite. Host layer only: shells and
/// components never import this.
///
/// A generated UI lives in `src/uis/<id>/` (docs/ARCHITECTURE.md,
/// "Generated UIs"): `ui.json` (what it answers), `main.ts` (mount,
/// then boot), `Shell.svelte` (placement only), `host.svelte.ts` (the
/// only file that asks the bus — through the engine, src/engine.ts),
/// and `<id>.html` at the web root.

/// The local date, "YYYY-MM-DD" — hosts own the clock; dates cross
/// every boundary as strings.
export function today(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

// ---------------------------------------------------------------------
// TRANSIENT — the answer graph, hand-built ahead of flow maps.
//
// The engine (src/engine.ts) makes every op call;
// docs/units/host-discipline.md has it interpret flow maps next (data
// in the UI manifest, docs/units/wire-time-assembly.md). Until then,
// a host declares what it answers as nodes and this runner schedules
// them. It is shaped to be REPLACED by flow maps, not grown into them:
//
// - a node is what a flow-map entry will be — the inputs it reads,
//   the frame that renders it, when that frame is on screen, and
//   the op calls — so a host's node list is the first draft of its
//   flow map (the engine's answers are already cached by the
//   journal's key);
// - it knows nothing about transit. If a node name, a field,
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
