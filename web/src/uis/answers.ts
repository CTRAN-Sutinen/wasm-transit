// The runner knows nothing about transit. A node name, a field, or a
// rule of the domain here is a leak. The file imports nothing, so its
// properties run under `node --test`.

export type FrameState =
  | { kind: "unassembled"; cap: string }
  | { kind: "starved"; missing: string }
  | { kind: "unread"; input: string; name: string; sinceMs: number }
  | { kind: "failed"; message: string };

export type AnswerNode = {
  // An op such as "analysis.service_groups", or "document.<name>" for a
  // document read before any op sees it.
  id: string;
  // "" for a document read, whose failure goes to the page's handler.
  frame: string;
  // A node may only name nodes declared before it.
  needs: string[];
  // While false, the node is not answered, `clear` runs, and its
  // dependents count as changed. Name the visibility input in `needs` so
  // showing the frame answers it.
  visible?: () => boolean;
  caps?: string[];
  // Name only a document that can be absent once the page has loaded,
  // never the one that the page boots with.
  requires?: () => string | null;
  // A held copy, or an input that keeps none, runs the node, and the shell
  // labels the copy's age.
  inputs?: string[];
  clear?: () => void;
  // The frame says it failed, over the last answer, which a board may keep
  // for a little while yet.
  keepOnFailure?: boolean;
  // `touch` marks an input that the node repaired, such as a selection that
  // the new answer no longer holds. Later nodes that read it re-answer in the
  // same pass.
  run: (touch: (input: string) => void) => Promise<void>;
};

// `frames` is a `$state` object, so the shell redraws.
export type AnswerHost = {
  frames: Record<string, FrameState>;
  onerror: (e: unknown) => void;
  available: (cap: string) => Promise<boolean>;
  inputs?: Record<string, InputSeen>;
};

// The full type is `InputState` in inputs.ts. This file imports nothing.
type InputSeen = {
  kind: string;
  input: string;
  name: string;
  sinceMs?: number;
  held?: boolean;
};

export type Request = ((...inputs: string[]) => Promise<void>) & {
  retry: () => Promise<void>;
};

const yieldToPaint = () => new Promise<void>((r) => setTimeout(r, 0));

function messageOf(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

// Yields between nodes so each frame paints as its answer lands. A request
// made while a pass runs supersedes it: the pass stops at the next node,
// and one merged pass starts from the top. Nodes already answered hit the
// engine's cache.
export function answers(nodes: AnswerNode[], host: AnswerHost): Request {
  const declared = new Set<string>();
  const ids = new Set(nodes.map((n) => n.id));
  for (const node of nodes) {
    for (const need of node.needs) {
      if (ids.has(need) && !declared.has(need))
        throw new Error(
          `answer node ${node.id} reads ${need} before it is declared`,
        );
    }
    declared.add(node.id);
  }

  // A frame shows the first of its nodes that has a state.
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

  async function blocked(node: AnswerNode): Promise<FrameState | null> {
    for (const cap of node.caps ?? []) {
      if (!(await host.available(cap))) return { kind: "unassembled", cap };
    }
    for (const id of node.inputs ?? []) {
      const seen = host.inputs?.[id];
      if (seen?.kind === "failed" && !seen.held)
        return {
          kind: "unread",
          input: id,
          name: seen.name,
          sinceMs: seen.sinceMs ?? 0,
        };
    }
    const missing = node.requires?.() ?? null;
    return missing ? { kind: "starved", missing } : null;
  }

  async function answer(
    node: AnswerNode,
    touch: (input: string) => void,
  ): Promise<void> {
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
      if (
        !node.needs.some((n) => changed.has(n)) &&
        !changed.has(`retry:${node.id}`)
      )
        continue;
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
    const failed = nodes
      .filter((n) => states.get(n.id)?.kind === "failed")
      .map((n) => `retry:${n.id}`);
    return request(...failed);
  };
  return request;
}
