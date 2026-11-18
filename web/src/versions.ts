/// A document a newer build wrote (D-79, D-186; docs/FORMATS.md,
/// "Rolling a version back"). A module refuses a version newer than
/// it reads, naming both (`transit_version::refused`), and refuses a
/// write over a copy it reads one version ahead through a read-only
/// step (`transit_version::write_refused`). The server's receive path
/// answers the same text. Either refusal means the page and the data
/// are on different builds. The page then asks for the deployment's
/// build: the page that registered a handler (kit's `installApp`) gets
/// it through its service worker, and applies it by its own rule.

/// The kind and version a refusal names, when it is one of the two.
export function newerRefusal(message: string): { kind: string; version: number } | null {
  const match = /(?:^|\b)([a-z_]+) v(\d+) (?:is newer than this module|was written by a newer build)/.exec(message);
  return match ? { kind: match[1], version: Number(match[2]) } : null;
}

const handlers = new Set<() => void>();

/// Run `handler` when a module or the server refuses a document as
/// written by another build.
export function onNewerBuild(handler: () => void): () => void {
  handlers.add(handler);
  return () => handlers.delete(handler);
}

/// Note a refusal's text: the engine passes every module error, and a
/// write passes every refused response. The text is answered as given.
export function noteRefusal(message: string): void {
  if (!newerRefusal(message)) return;
  for (const handler of handlers) handler();
}
