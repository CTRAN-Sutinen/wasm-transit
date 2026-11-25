// A module error is `{message}`, and a refusal that a host acts on also carries
// a code and its fields (docs/architecture/bus.md, "Answers"). Hosts branch on
// the code, never on the message, which is text for a person.

import type { RefusalCode } from "./types/transit_version/RefusalCode";

export type ModuleError = { message: string } & (RefusalCode | { code?: undefined });

export class Refused extends Error {
  readonly error: ModuleError;
  constructor(error: ModuleError) {
    super(error.message);
    this.error = error;
  }
}

// Reads an error from a thrown Refused, an outcome, or the text of a server's
// answer (`{"error": {...}}`).
export function moduleError(value: unknown): ModuleError | null {
  if (value instanceof Refused) return value.error;
  if (typeof value === "string") {
    if (!value.startsWith('{"error"')) return null;
    try {
      return moduleError(JSON.parse(value));
    } catch {
      return null;
    }
  }
  if (value && typeof value === "object" && "error" in value) {
    const error = (value as { error: unknown }).error;
    if (error && typeof error === "object" && typeof (error as ModuleError).message === "string") return error as ModuleError;
  }
  return null;
}

// The hash of a document the module no longer holds, which the engine registers
// again.
export function unknownDocument(value: unknown): string | null {
  const error = moduleError(value);
  return error?.code === "unknown_document" ? error.hash : null;
}

// A document that a newer build wrote, which asks for the deployment's build.
export function newerVersion(value: unknown): { kind: string; version: number } | null {
  const error = moduleError(value);
  return error?.code === "newer_version" ? { kind: error.kind, version: error.found } : null;
}
