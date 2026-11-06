/// The service-design components' formatting: clock times, minutes,
/// and counts — presentation only, never a domain rule.

/// Seconds since service-day midnight as "H:MM" (after-midnight
/// times read past 24:00, as the schedule runs them).
export function clock(secs: number | null | undefined): string {
  if (secs == null) return "—";
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return `${h}:${String(m).padStart(2, "0")}`;
}

/// Seconds as whole minutes ("12"), or "—".
export function mins(secs: number | null | undefined): string {
  return secs == null ? "—" : String(Math.round(secs / 60));
}

export function count(n: number | null | undefined): string {
  return n == null ? "—" : n.toLocaleString();
}

export function fixed(n: number | null | undefined, digits = 1): string {
  return n == null ? "—" : n.toLocaleString(undefined, { minimumFractionDigits: digits, maximumFractionDigits: digits });
}

/// "a → b" when the two differ, else "b".
export function change(a: string, b: string): string {
  return a === b ? b : `${a} → ${b}`;
}

/// Parse "H:MM" into seconds, or null.
export function parseClock(text: string): number | null {
  const m = text.trim().match(/^(\d{1,2}):(\d{2})$/);
  return m ? Number(m[1]) * 3600 + Number(m[2]) * 60 : null;
}

/// A combination's dates by its spans (analysis `service_groups`):
/// "2026-09-14 – 2026-10-09 (20 days)", a one-date span as its date.
export function spansText(spans: { start: string; end: string; days: number }[]): string {
  return spans.map((s) => (s.start === s.end ? s.start : `${s.start} – ${s.end} (${s.days} days)`)).join(", ");
}
