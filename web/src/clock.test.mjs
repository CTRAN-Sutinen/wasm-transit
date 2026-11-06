// The service clock (src/clock.ts): an instant read the way GTFS
// counts a service day, in the agency's zone — never the device's.
// The GTFS spec: a stop time counts from "noon minus 12h" on the
// service date, local time, which is midnight except on the days the
// clocks change. C-TRAN's zone is America/Los_Angeles: in 2026 the
// clocks go forward on 2026-03-08 and back on 2026-11-01.
//
// Run: node --test web/src/clock.test.mjs (`just test`, web gates).
import { test } from "node:test";
import assert from "node:assert/strict";
import { serviceClock } from "./clock.ts";

const LA = "America/Los_Angeles";
const at = (iso) => Date.parse(iso);

test("an ordinary afternoon is the agency's date and seconds since midnight", () => {
  assert.deepEqual(serviceClock(at("2026-11-06T15:30:00-08:00"), LA), { date: "2026-11-06", secs: 55800 });
});

test("the agency's zone, not the device's: the same instant reads alike from any zone", () => {
  // 07:30 UTC on the 7th is still the 6th in Vancouver, WA.
  assert.deepEqual(serviceClock(at("2026-11-07T07:30:00Z"), LA), { date: "2026-11-06", secs: 84600 });
  assert.deepEqual(serviceClock(at("2026-11-07T07:30:00Z"), "Asia/Tokyo"), { date: "2026-11-07", secs: 59400 });
});

test("just after midnight is the new date", () => {
  assert.deepEqual(serviceClock(at("2026-11-07T00:30:00-08:00"), LA), { date: "2026-11-07", secs: 1800 });
});

test("spring forward: the day counts from noon minus 12h (23:00 the evening before)", () => {
  // 01:30 PST is 2.5 h after the anchor, though the wall reads 1:30.
  assert.deepEqual(serviceClock(at("2026-03-08T01:30:00-08:00"), LA), { date: "2026-03-08", secs: 9000 });
  // After the jump the wall and the anchor agree.
  assert.deepEqual(serviceClock(at("2026-03-08T03:00:00-07:00"), LA), { date: "2026-03-08", secs: 10800 });
  assert.deepEqual(serviceClock(at("2026-03-08T12:00:00-07:00"), LA), { date: "2026-03-08", secs: 43200 });
});

test("fall back: the day counts from noon minus 12h (01:00 PDT)", () => {
  // The first 01:30 (PDT) and the second (PST) are an hour apart.
  assert.deepEqual(serviceClock(at("2026-11-01T01:30:00-07:00"), LA), { date: "2026-11-01", secs: 1800 });
  assert.deepEqual(serviceClock(at("2026-11-01T01:30:00-08:00"), LA), { date: "2026-11-01", secs: 5400 });
  assert.deepEqual(serviceClock(at("2026-11-01T12:00:00-08:00"), LA), { date: "2026-11-01", secs: 43200 });
  assert.deepEqual(serviceClock(at("2026-11-01T23:00:00-08:00"), LA), { date: "2026-11-01", secs: 82800 });
});

test("fall back: the hour before the anchor clamps to the day's start", () => {
  // 00:30 PDT precedes 2026-11-01's anchor; seconds cross the
  // boundary unsigned, so the clock answers the day's first second
  // (the previous service day's after-midnight trips are
  // docs/units/deploy-rider-apps.md's).
  assert.deepEqual(serviceClock(at("2026-11-01T00:30:00-07:00"), LA), { date: "2026-11-01", secs: 0 });
});

test("an unknown or absent zone reads the device's", () => {
  const now = at("2026-11-06T15:30:00Z");
  assert.deepEqual(serviceClock(now, "Not/AZone"), serviceClock(now, undefined));
});
