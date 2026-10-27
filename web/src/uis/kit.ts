/// The generated UIs' shared host glue — what every UI's host layer
/// needs and none should rewrite. Host layer only: shells and
/// components never import this (it reaches the resolver).
///
/// A generated UI lives in `src/uis/<id>/` (docs/ARCHITECTURE.md,
/// "Generated UIs"): `ui.json` (what it answers), `main.ts` (boot +
/// mount), `Shell.svelte` (placement only), `host.svelte.ts` (the
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
