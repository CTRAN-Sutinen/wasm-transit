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

/// An issue or warning as words (network `validate`, cap-author's
/// `apply`, the server's publish refusals): its kind as a title, and
/// its fields as a sentence a planner reads — times as clock times,
/// waits as minutes. Tagged by a `kind` field or by its one key. The
/// kinds a planner meets while changing service have their own
/// sentence; every other kind lists its fields as given.
export function issueWords(item: object): { kind: string; detail: string } {
  if ("kind" in item && typeof item.kind === "string") {
    const { kind, ...rest } = item as Record<string, unknown>;
    return issueWords({ [kind as string]: rest });
  }
  const [tag, body] = Object.entries(item)[0] ?? ["", {}];
  const f = (body ?? {}) as Record<string, unknown>;
  const secs = (k: string) => (typeof f[k] === "number" ? (f[k] as number) : null);
  const kind = tag.replace(/_/g, " ");
  switch (tag) {
    case "block_infeasible":
      return { kind, detail: `Block ${f.block}: trip ${f.trip} arrives at ${clock(secs("arrival_secs"))}, after its next trip ${f.next} leaves at ${clock(secs("start_secs"))}. One bus cannot run both.` };
    case "layover_infeasible":
      // `after_trip` is the trip's place in the block, counted from 0.
      return { kind, detail: `Block ${f.block}: the wait after trip number ${Number(f.after_trip) + 1} of the block is ${mins(secs("gap_secs"))} min; ${mins(secs("required_secs"))} min is required there.` };
    case "unassigned_trips":
      return { kind, detail: `${f.trips} ${f.trips === 1 ? "trip" : "trips"} of ${f.pattern_trips} ${f.trips === 1 ? "is" : "are"} on no block, so no vehicle is counted for ${f.trips === 1 ? "it" : "them"}.` };
    case "no_service_in_plan_period":
      return { kind, detail: `${f.plan_name} picks on ${f.effective_from}, and no service type runs from that day. Fork with a "runs through" date to carry the calendar.` };
  }
  const detail = Object.entries(f)
    .map(([k, v]) => `${k.replace(/_/g, " ")} ${typeof v === "object" ? JSON.stringify(v) : v}`)
    .join(", ");
  return { kind, detail };
}
