/// Formatting for the rider's components — strings from op answers,
/// nothing decided. Times are service-day seconds (past 86400 is an
/// after-midnight trip of the same day: it reads as the clock time).

import type { LiveStatus } from "../types/LiveStatus";

/// "8:02" and "am" — a rider's clock, the suffix set apart so a
/// board can size it down.
export function clock(secs: number): { hm: string; ap: string } {
  const h24 = Math.floor(secs / 3600) % 24;
  const m = Math.floor((secs % 3600) / 60);
  const h = h24 % 12 === 0 ? 12 : h24 % 12;
  return { hm: `${h}:${String(m).padStart(2, "0")}`, ap: h24 < 12 ? "am" : "pm" };
}

export function clockText(secs: number): string {
  const c = clock(secs);
  return `${c.hm} ${c.ap}`;
}

function amount(secs: number): string {
  const abs = Math.abs(secs);
  return abs < 60 ? `${abs} s` : `${Math.round(abs / 60)} min`;
}

/// The words for a row's status under the live document's window.
export function statusText(status: LiveStatus, delaySecs: number | null): string {
  switch (status) {
    case "on_time":
      return "On time";
    case "early":
      return `${amount(delaySecs ?? 0)} early`;
    case "late":
    case "very_late":
      return `${amount(delaySecs ?? 0)} late`;
    default:
      return "Scheduled";
  }
}

/// The tone a status renders in. Early is a missed bus — it reads as
/// bad as very late, never as on time.
export function statusTone(status: LiveStatus): "ok" | "warn" | "bad" | "sched" {
  switch (status) {
    case "on_time":
      return "ok";
    case "late":
      return "warn";
    case "early":
    case "very_late":
      return "bad";
    default:
      return "sched";
  }
}

/// A leg's predicted time against the day's own.
export function delayText(delaySecs: number | null): string | null {
  if (delaySecs == null || delaySecs === 0) return null;
  return delaySecs < 0 ? `${amount(delaySecs)} early` : `${amount(delaySecs)} late`;
}

export function distanceText(m: number): string {
  return m < 1000 ? `${Math.round(m / 10) * 10} m` : `${(m / 1000).toFixed(1)} km`;
}

export function walkText(secs: number): string {
  return `${Math.max(1, Math.round(secs / 60))} min walk`;
}

/// A route's authored color as CSS (null without one).
export function routeColor(color: string | null | undefined): string | null {
  return color ? `#${color.replace(/^#/, "")}` : null;
}
