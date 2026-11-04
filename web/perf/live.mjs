// The recorded live feeds a perf spec replays (docs/TESTING.md,
// "Performance baselines"): `node web/perf/record-rt.mjs` writes
// <dir>/<n>-<kind>.pb; a spec serves snapshot n at poll n.

import fs from "node:fs";
import path from "node:path";

/// The recorded snapshots, in order: [{ trips, vehicles }] as bytes.
export function snapshots(rtDir) {
  const byN = new Map();
  for (const f of fs.readdirSync(rtDir).sort()) {
    const m = /^(\d+)-(\w+)\.pb$/.exec(f);
    if (!m) continue;
    if (!byN.has(m[1])) byN.set(m[1], {});
    byN.get(m[1])[m[2]] = fs.readFileSync(path.join(rtDir, f));
  }
  return [...byN.values()];
}

/// The FeedHeader's timestamp (FeedMessage field 1, header field 3):
/// the clock starts where the recording did.
export function headerSecs(buf) {
  const varint = (i) => {
    let x = 0, s = 1;
    for (;;) {
      const c = buf[i++];
      x += (c & 0x7f) * s;
      s *= 128;
      if (!(c & 0x80)) return [x, i];
    }
  };
  let [tag, i] = varint(0);
  if (tag !== 0x0a) throw new Error("no FeedHeader");
  let len;
  [len, i] = varint(i);
  const end = i + len;
  while (i < end) {
    let t, v;
    [t, i] = varint(i);
    if ((t & 7) === 0) {
      [v, i] = varint(i);
      if (t >> 3 === 3) return v;
    } else if ((t & 7) === 2) {
      [v, i] = varint(i);
      i += v;
    } else throw new Error(`unexpected wire type in FeedHeader`);
  }
  throw new Error("FeedHeader has no timestamp");
}
