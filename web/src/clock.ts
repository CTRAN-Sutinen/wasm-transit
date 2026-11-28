// The zone is the agency's (the manifest's `timezone`), never the
// device's. The agent runner (`tools/caps-run.mjs`) reads `$TODAY` and
// `$NOW_SECS` through this file too. It has no imports, so
// `node --test web/src/clock.test.mjs` runs it bare.
//
// Seconds count from noon minus 12 h on the date (the GTFS anchor), which
// is midnight except on the days the clocks change. The hour of a
// fall-back day before its anchor clamps to 0, because seconds cross
// every boundary unsigned. `previousClock` answers that hour truly.

export type ServiceClock = { date: string; secs: number };

export function serviceClock(nowMs: number, zone?: string): ServiceClock {
  const tz = known(zone);
  const [y, m, d] = wall(nowMs, tz);
  const secs = Math.floor((nowMs - anchor(y, m, d, tz)) / 1000);
  return { date: isoDate(y, m, d), secs: Math.max(0, secs) };
}

// The previous service date at the same instant, so a board can read the
// after-midnight trips that are still leaving. `length_secs` is 90000 the
// night the clocks go back and 82800 going forward.
export type PreviousClock = {
  date: string;
  from_secs: number;
  length_secs: number;
};

export function previousClock(nowMs: number, zone?: string): PreviousClock {
  const tz = known(zone);
  const [y, m, d] = wall(nowMs, tz);
  const prev = new Date(Date.UTC(y, m - 1, d - 1));
  const [py, pm, pd] = [
    prev.getUTCFullYear(),
    prev.getUTCMonth() + 1,
    prev.getUTCDate(),
  ];
  const start = anchor(py, pm, pd, tz);
  return {
    date: isoDate(py, pm, pd),
    from_secs: Math.floor((nowMs - start) / 1000),
    length_secs: Math.round((anchor(y, m, d, tz) - start) / 1000),
  };
}

function anchor(y: number, m: number, d: number, tz?: string): number {
  return zonedUtc(y, m, d, 12, tz) - 43_200_000;
}

function isoDate(y: number, m: number, d: number): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${y}-${pad(m)}-${pad(d)}`;
}

function known(zone?: string): string | undefined {
  if (!zone) return undefined;
  try {
    new Intl.DateTimeFormat("en-US", { timeZone: zone });
    return zone;
  } catch {
    return undefined;
  }
}

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
  return [
    get("year"),
    get("month"),
    get("day"),
    get("hour"),
    get("minute"),
    get("second"),
  ];
}

function offset(ms: number, tz?: string): number {
  const [y, m, d, h, mi, s] = wall(ms, tz);
  return Date.UTC(y, m - 1, d, h, mi, s) - (ms - (((ms % 1000) + 1000) % 1000));
}

// Noon is never in a clock change, so one correction settles it.
function zonedUtc(
  y: number,
  m: number,
  d: number,
  h: number,
  tz?: string,
): number {
  const guess = Date.UTC(y, m - 1, d, h);
  const first = guess - offset(guess, tz);
  return guess - offset(first, tz);
}
