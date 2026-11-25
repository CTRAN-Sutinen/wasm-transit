import { newerVersion } from "./refusals.ts";

const handlers = new Set<() => void>();

export function onNewerBuild(handler: () => void): () => void {
  handlers.add(handler);
  return () => handlers.delete(handler);
}

/// The engine passes every module error, and a write passes every refused
/// response's body (src/refusals.ts).
export function noteRefusal(error: unknown): void {
  if (!newerVersion(error)) return;
  for (const handler of handlers) handler();
}
