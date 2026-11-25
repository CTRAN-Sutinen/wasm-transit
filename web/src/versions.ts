/// Matches the two refusals for a document that a newer build wrote
/// (`transit_version::refused`, `transit_version::write_refused`; D-79,
/// D-186; docs/FORMATS.md, "Rolling a version back"). The server's write
/// path answers the same text.
export function newerRefusal(message: string): { kind: string; version: number } | null {
  const match = /(?:^|\b)([a-z_]+) v(\d+) (?:is newer than this module|was written by a newer build)/.exec(message);
  return match ? { kind: match[1], version: Number(match[2]) } : null;
}

const handlers = new Set<() => void>();

export function onNewerBuild(handler: () => void): () => void {
  handlers.add(handler);
  return () => handlers.delete(handler);
}

/// The engine passes every module error, and a write passes every refused
/// response.
export function noteRefusal(message: string): void {
  if (!newerRefusal(message)) return;
  for (const handler of handlers) handler();
}
