/// The answer graph. A UI's flows are code: a host declares what it
/// answers as nodes, and this runner schedules them (D-176). The engine
/// (src/engine.ts) makes every op call, and caches each answer under
/// the journal's key. Hosts reach it through `answerGraph` in kit.ts,
/// which gives it the engine's `available`.
///
/// - A node is the inputs it reads, the frame that renders it, when
///   that frame is on screen, the capabilities and documents it needs,
///   and the op calls.
/// - The graph keeps each node's state. A failure belongs to its frame,
///   and every other node answers (laws: answers.test.mjs).
/// - The runner knows nothing about transit. If a node name, a field,
///   or a rule of the domain appears here, it leaked.
///
/// This file imports nothing, so its laws run under `node --test`.

/// Why a frame shows no answer. A frame that answered has no state.
export type FrameState =
  /// A capability its node asks did not assemble.
  | { kind: "unassembled"; cap: string }
  /// A document its node reads is not there.
  | { kind: "starved"; missing: string }
  /// An input its node requires (src/uis/inputs.ts) has failed since
  /// `sinceMs`, and the page holds nothing of it.
  | { kind: "unread"; input: string; name: string; sinceMs: number }
  /// Its node's op refused, or the node threw. `retry` asks again.
  | { kind: "failed"; message: string };

/// One answer a host keeps: an op (or a short run of ops over the
/// same inputs) and the host state it writes.
export type AnswerNode = {
  /// The op(s) it answers ("analysis.service_groups"), or
  /// "document.<name>" for a document read before any op sees it.
  id: string;
  /// The frame that renders the answer, as the host's state names it
  /// ("" for a document read: its failure goes to the page's handler).
  frame: string;
  /// The inputs (selection keys, document keys) and earlier nodes'
  /// ids it reads: a change to any of them re-answers it. A node may
  /// only name nodes declared before it.
  needs: string[];
  /// False while its frame is off screen: the node is not answered,
  /// `clear` runs, the frame has no state, and its dependents count as
  /// changed. Name the visibility input in `needs` so showing the
  /// frame answers it.
  visible?: () => boolean;
  /// The capabilities its ops ask. One that did not assemble makes the
  /// node unassembled, and it is not run.
  caps?: string[];
  /// The name of a document it reads that is not there ("the
  /// baseline"), or null. The node is then starved, and it is not run.
  /// Name only a document that can be absent once the page has loaded,
  /// never the one the page boots with.
  requires?: () => string | null;
  /// The inputs (src/uis/inputs.ts) it cannot answer without. While
  /// one's reads fail and the page holds nothing of it, the node is
  /// unread, and it is not run. A held copy, or an input that keeps
  /// none, runs it: the shell labels the copy's age.
  inputs?: string[];
  /// Clear what it wrote: when it is hidden, unassembled, starved, or
  /// failed.
  clear?: () => void;
  /// On a failure, keep what it wrote: the frame says it failed, over
  /// the last answer (a board right for a little while yet).
  keepOnFailure?: boolean;
  /// Answer, write the host state, and `touch` any input it repaired
  /// (a selection the new answer no longer holds). Later nodes that
  /// read that input re-answer in the same pass.
  run: (touch: (input: string) => void) => Promise<void>;
};

/// What the graph needs from its host: the host's record of frame
/// states (a `$state` object, so the shell redraws), the page's handler
/// for a document read that fails, and whether a capability assembled.
export type AnswerHost = {
  frames: Record<string, FrameState>;
  onerror: (e: unknown) => void;
  available: (cap: string) => Promise<boolean>;
  /// The inputs' states (src/uis/inputs.ts, `InputState`), by id.
  inputs?: Record<string, InputSeen>;
};

/// What the graph reads of an input's state. The full type is in
/// inputs.ts; this file imports nothing.
type InputSeen = { kind: string; input: string; name: string; sinceMs?: number; held?: boolean };

/// `request(...inputs)` marks inputs changed and answers every node
/// downstream of them. `request.retry()` answers again each node that
/// failed.
export type Request = ((...inputs: string[]) => Promise<void>) & { retry: () => Promise<void> };

const yieldToPaint = () => new Promise<void>((r) => setTimeout(r, 0));

function messageOf(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/// The runner. A request answers every node downstream of its inputs,
/// in declared order, yielding between nodes so each frame paints as
/// its answer lands. A request made while a pass runs supersedes it:
/// the pass stops at the next node, and one merged pass starts from the
/// top (nodes already answered hit the engine's cache). The returned
/// promise settles when no request is left.
export function answers(nodes: AnswerNode[], host: AnswerHost): Request {
  const declared = new Set<string>();
  const ids = new Set(nodes.map((n) => n.id));
  for (const node of nodes) {
    for (const need of node.needs) {
      if (ids.has(need) && !declared.has(need)) throw new Error(`answer node ${node.id} reads ${need} before it is declared`);
    }
    declared.add(node.id);
  }

  // Each node's state; a frame shows the first of its nodes that has one.
  const states = new Map<string, FrameState>();
  const show = (frame: string) => {
    const first = nodes.find((n) => n.frame === frame && states.has(n.id));
    if (first) host.frames[frame] = states.get(first.id)!;
    else delete host.frames[frame];
  };
  const settle = (node: AnswerNode, state: FrameState | null) => {
    if (state) states.set(node.id, state);
    else states.delete(node.id);
    show(node.frame);
  };

  // The state a node is in before it runs, or null when it may run.
  async function blocked(node: AnswerNode): Promise<FrameState | null> {
    for (const cap of node.caps ?? []) {
      if (!(await host.available(cap))) return { kind: "unassembled", cap };
    }
    for (const id of node.inputs ?? []) {
      const seen = host.inputs?.[id];
      if (seen?.kind === "failed" && !seen.held) return { kind: "unread", input: id, name: seen.name, sinceMs: seen.sinceMs ?? 0 };
    }
    const missing = node.requires?.() ?? null;
    return missing ? { kind: "starved", missing } : null;
  }

  async function answer(node: AnswerNode, touch: (input: string) => void): Promise<void> {
    if (node.frame === "") {
      try {
        await node.run(touch);
      } catch (e) {
        host.onerror(e);
      }
      return;
    }
    const before = await blocked(node);
    if (before) {
      node.clear?.();
      return settle(node, before);
    }
    try {
      await node.run(touch);
      settle(node, null);
    } catch (e) {
      if (!node.keepOnFailure) node.clear?.();
      console.error(node.id, e);
      settle(node, { kind: "failed", message: messageOf(e) });
    }
  }

  let pending = new Set<string>();
  let draining: Promise<void> | null = null;

  async function pass(changed: Set<string>): Promise<void> {
    for (const node of nodes) {
      if (!node.needs.some((n) => changed.has(n)) && !changed.has(`retry:${node.id}`)) continue;
      if (node.visible && !node.visible()) {
        node.clear?.();
        if (node.frame !== "") settle(node, null);
        changed.add(node.id);
        continue;
      }
      await yieldToPaint();
      if (pending.size) {
        for (const c of changed) pending.add(c);
        return;
      }
      await answer(node, (input) => changed.add(input));
      changed.add(node.id);
    }
  }

  const request = function request(...inputs: string[]): Promise<void> {
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
  } as Request;
  request.retry = () => {
    const failed = nodes.filter((n) => states.get(n.id)?.kind === "failed").map((n) => `retry:${n.id}`);
    return request(...failed);
  };
  return request;
}
