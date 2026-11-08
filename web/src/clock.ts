/// The service clock — hosts own the clock (AGENTS.md, ownership
/// rules), and this is how a page reads it: an instant read in the
/// AGENCY's zone (the manifest's `timezone`, the network's operator
/// zone, as the server counts its own today), never the device's —
/// a phone or a display set to another zone answers the same service
/// day. No imports: `node --test web/src/clock.test.mjs` runs it bare.
///
/// The date is the agency's calendar date; the seconds count from
/// noon minus 12 h on that date (the GTFS spec's anchor), which is
/// midnight except on the days the clocks change. The hour of a
/// fall-back day before its anchor clamps to 0 — seconds cross every
/// boundary unsigned; `previousClock` answers that hour truly, on the
/// previous date's clock.

export type ServiceClock = { date: string; secs: number };

export function serviceClock(nowMs: number, zone?: string): ServiceClock {
  const tz = known(zone);
  const [y, m, d] = wall(nowMs, tz);
  const secs = Math.floor((nowMs - anchor(y, m, d, tz)) / 1000);
  return { date: isoDate(y, m, d), secs: Math.max(0, secs) };
}

/// The previous service date at the same instant: the instant on ITS
/// clock (past 86 400 after midnight — its after-midnight trips) and
/// its length anchor to anchor (86 400; 90 000 the night the clocks go
/// back, 82 800 going forward). A board reads that date's trips still
/// to leave from it, and the true instant in the hour before a
/// fall-back day's anchor (`serviceClock` clamps it to 0).
export type PreviousClock = { date: string; from_secs: number; length_secs: number };

export function previousClock(nowMs: number, zone?: string): PreviousClock {
  const tz = known(zone);
  const [y, m, d] = wall(nowMs, tz);
  const prev = new Date(Date.UTC(y, m - 1, d - 1));
  const [py, pm, pd] = [prev.getUTCFullYear(), prev.getUTCMonth() + 1, prev.getUTCDate()];
  const start = anchor(py, pm, pd, tz);
  return {
    date: isoDate(py, pm, pd),
    from_secs: Math.floor((nowMs - start) / 1000),
    length_secs: Math.round((anchor(y, m, d, tz) - start) / 1000),
  };
}

/// A service date's anchor: noon minus 12 h, local time (GTFS).
function anchor(y: number, m: number, d: number, tz?: string): number {
  return zonedUtc(y, m, d, 12, tz) - 43_200_000;
}

function isoDate(y: number, m: number, d: number): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${y}-${pad(m)}-${pad(d)}`;
}

/// The zone, when the runtime's tz database knows it.
function known(zone?: string): string | undefined {
  if (!zone) return undefined;
  try {
    new Intl.DateTimeFormat("en-US", { timeZone: zone });
    return zone;
  } catch {
    return undefined;
  }
}

/// The wall clock in `tz` at an instant: year, month, day, hour,
/// minute, second.
function wall(ms: number, tz?: string): number[] {
  const parts = new Intl.DateTimeFormat("en-US", {
    timeZone: tz,
    hourCycle: "h23",
    year: "numeric",
    month: "numeric",
    day: "numeric",
    hour: "numeric",
    minute: "numeric",
    second: "numeric",
  }).formatToParts(ms);
  const get = (t: string) => Number(parts.find((p) => p.type === t)?.value);
  return [get("year"), get("month"), get("day"), get("hour"), get("minute"), get("second")];
}

/// The zone's offset from UTC at an instant, in ms (wall minus UTC).
function offset(ms: number, tz?: string): number {
  const [y, m, d, h, mi, s] = wall(ms, tz);
  return Date.UTC(y, m - 1, d, h, mi, s) - (ms - (((ms % 1000) + 1000) % 1000));
}

/// The instant the wall clock in `tz` reads `h`:00 on y-m-d (noon is
/// never in a clock change, so one correction settles it).
function zonedUtc(y: number, m: number, d: number, h: number, tz?: string): number {
  const guess = Date.UTC(y, m - 1, d, h);
  const first = guess - offset(guess, tz);
  return guess - offset(first, tz);
}
