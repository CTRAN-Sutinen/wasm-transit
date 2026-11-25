/// Times are service-day seconds. Past 86400 is an after-midnight trip of the
/// same day and reads as the clock time.

import type { LiveStatus } from "../types/LiveStatus";
import type { Wait } from "../types/Wait";
import type { NextService } from "../types/NextService";

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

/// Early is a missed bus, so it reads as bad as very late and never as on time.
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

export function routeColor(color: string | null | undefined): string | null {
  return color ? `#${color.replace(/^#/, "")}` : null;
}

export function waitText(wait: Wait): string {
  switch (wait.kind) {
    case "due":
      return "Due";
    case "minutes":
      return `${wait.minutes} min`;
    default:
      return clockText(wait.secs);
  }
}

/// Follows the board's wait rule (D-40).
export function leaveText(wait: Wait): string {
  switch (wait.kind) {
    case "due":
      return "Leave now";
    case "minutes":
      return `Leave in ${wait.minutes} min`;
    default:
      return `Leave at ${clockText(wait.secs)}`;
  }
}

export function durationText(secs: number): string {
  const m = Math.round(secs / 60);
  return m < 60 ? `${m} min` : `${Math.floor(m / 60)} h${m % 60 ? ` ${m % 60} min` : ""}`;
}

/// Follows the board's `next` (D-42).
export function nextText(next: NextService): string {
  const at = clockText(next.departure_secs);
  return next.days_ahead === 1 ? `Next: ${at} tomorrow` : `Next: ${next.weekday} ${at}`;
}
