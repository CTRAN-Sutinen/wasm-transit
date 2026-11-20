/// Formatting for the disruptions components: strings from op answers,
/// nothing decided. Service-day times are seconds after the day's
/// anchor (past 86400 is an after-midnight trip of the same day). Note
/// times are Unix seconds, shown in the agency's zone.

import type { Disruption } from "../types/Disruption";
import type { Origin } from "../types/Origin";

/// "2:10 PM": a service-day time.
export function timeText(secs: number): string {
  const h24 = Math.floor(secs / 3600) % 24;
  const m = Math.floor((secs % 3600) / 60);
  const h = h24 % 12 === 0 ? 12 : h24 % 12;
  return `${h}:${String(m).padStart(2, "0")} ${h24 < 12 ? "AM" : "PM"}`;
}

/// "2:26 PM", or "Nov 18, 2:26 PM" when not on `date`: a Unix time in
/// the agency's zone.
export function stampText(unix: number, zone: string | undefined, date?: string): string {
  const at = new Date(unix * 1000);
  const opts = (o: Intl.DateTimeFormatOptions) => {
    try {
      return at.toLocaleString("en-US", { ...o, timeZone: zone });
    } catch {
      return at.toLocaleString("en-US", o);
    }
  };
  const time = opts({ hour: "numeric", minute: "2-digit" });
  const day = opts({ year: "numeric", month: "2-digit", day: "2-digit" });
  const [mm, dd, yyyy] = day.split("/");
  if (!date || `${yyyy}-${mm}-${dd}` === date) return time;
  return `${opts({ month: "short", day: "numeric" })}, ${time}`;
}

/// "Nov 24": a "YYYY-MM-DD" date.
export function dateText(date: string): string {
  const d = new Date(`${date}T12:00:00Z`);
  return d.toLocaleDateString("en-US", { month: "short", day: "numeric", timeZone: "UTC" });
}

/// "9 min": a delay.
export function minutesText(secs: number): string {
  return `${Math.max(1, Math.round(secs / 60))} min`;
}

/// Where a disruption came from.
export function originText(from: Origin): string {
  switch (from) {
    case "feed":
      return "CAD feed";
    case "edits":
      return "Dispatch";
    default:
      return "Stops";
  }
}

/// What a disruption is, in a word.
export function kindText(d: Disruption): string {
  if (d.from === "stops") return "Stop closed";
  if (d.effect === "detour") return "Detour";
  if (d.trip) return "Cancelled";
  return "No service";
}

/// The disruption's title line: "2:10 PM from Fisher's Landing TC",
/// "Broadway & 25th", "Skips 4 stops".
export function titleText(d: Disruption): string {
  if (d.from === "stops") return d.stops?.[0]?.name ?? d.stops?.[0]?.id ?? "";
  if (d.trip) {
    const start = d.start_secs != null ? timeText(d.start_secs) : "Every trip";
    const from = d.first_stop ? ` from ${d.first_stop.name ?? d.first_stop.id}` : "";
    return `${start}${from}`;
  }
  const n = d.stops?.length ?? 0;
  return n ? `Skips ${n} ${n === 1 ? "stop" : "stops"}` : "No stops skipped";
}

/// The dates a disruption is in force: "Until Nov 24", "Nov 18 – Nov 20",
/// or "" for today alone.
export function datesText(d: Disruption, today: string): string {
  if (d.start_date && d.start_date === d.end_date) return d.start_date === today ? "" : dateText(d.start_date);
  if (d.start_date && d.end_date) return `${dateText(d.start_date)} – ${dateText(d.end_date)}`;
  if (d.end_date) return `Until ${dateText(d.end_date)}`;
  if (d.start_date) return `Since ${dateText(d.start_date)}`;
  return "";
}

/// A note's writer: the name part of an email, else as given.
export function whoText(by: string): string {
  return by.includes("@") ? by.slice(0, by.indexOf("@")) : by;
}

/// A Unix time as the agency's date and wall time: "YYYY-MM-DD" and
/// "HH:MM".
export function zonedParts(unix: number, zone: string | undefined): { date: string; time: string } {
  const parts = new Intl.DateTimeFormat("en-CA", {
    timeZone: zone,
    hourCycle: "h23",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).formatToParts(new Date(unix * 1000));
  const get = (t: string) => parts.find((p) => p.type === t)?.value ?? "";
  return { date: `${get("year")}-${get("month")}-${get("day")}`, time: `${get("hour")}:${get("minute")}` };
}

/// The agency's date and wall time as a Unix time.
export function zonedUnix(date: string, time: string, zone: string | undefined): number {
  const guess = Date.parse(`${date}T${time || "00:00"}:00Z`) / 1000;
  const seen = zonedParts(guess, zone);
  const offset = Date.parse(`${seen.date}T${seen.time}:00Z`) / 1000 - guess;
  return guess - offset;
}

/// "Nov 19, 2:32 PM": a Unix time in the agency's zone.
export function whenText(unix: number | null, zone: string | undefined): string {
  if (unix == null) return "No end";
  const { date } = zonedParts(unix, zone);
  return `${dateText(date)}, ${stampText(unix, zone, date)}`;
}

/// How long an alert has been told to riders: "Open 3 hours", "Open 41
/// days".
export function openText(secs: number): string {
  const days = Math.floor(secs / 86400);
  if (days >= 1) return `Open ${days} ${days === 1 ? "day" : "days"}`;
  const hours = Math.floor(secs / 3600);
  if (hours >= 1) return `Open ${hours} ${hours === 1 ? "hour" : "hours"}`;
  return `Open ${Math.max(1, Math.floor(secs / 60))} min`;
}
