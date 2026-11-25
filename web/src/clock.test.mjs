// Pins the service clock to the GTFS rule that a stop time counts from
// noon minus 12 h on the service date. In America/Los_Angeles the clocks
// go forward on 2026-03-08 and back on 2026-11-01.
import { test } from "node:test";
import assert from "node:assert/strict";
import { previousClock, serviceClock } from "./clock.ts";

const LA = "America/Los_Angeles";
const at = (iso) => Date.parse(iso);

test("an ordinary afternoon is the agency's date and seconds since midnight", () => {
  assert.deepEqual(serviceClock(at("2026-11-06T15:30:00-08:00"), LA), { date: "2026-11-06", secs: 55800 });
});

test("the agency's zone, not the device's: the same instant reads alike from any zone", () => {
  assert.deepEqual(serviceClock(at("2026-11-07T07:30:00Z"), LA), { date: "2026-11-06", secs: 84600 });
  assert.deepEqual(serviceClock(at("2026-11-07T07:30:00Z"), "Asia/Tokyo"), { date: "2026-11-07", secs: 59400 });
});

test("just after midnight is the new date", () => {
  assert.deepEqual(serviceClock(at("2026-11-07T00:30:00-08:00"), LA), { date: "2026-11-07", secs: 1800 });
});

test("spring forward: the day counts from noon minus 12h (23:00 the evening before)", () => {
  assert.deepEqual(serviceClock(at("2026-03-08T01:30:00-08:00"), LA), { date: "2026-03-08", secs: 9000 });
  assert.deepEqual(serviceClock(at("2026-03-08T03:00:00-07:00"), LA), { date: "2026-03-08", secs: 10800 });
  assert.deepEqual(serviceClock(at("2026-03-08T12:00:00-07:00"), LA), { date: "2026-03-08", secs: 43200 });
});

test("fall back: the day counts from noon minus 12h (01:00 PDT)", () => {
  assert.deepEqual(serviceClock(at("2026-11-01T01:30:00-07:00"), LA), { date: "2026-11-01", secs: 1800 });
  assert.deepEqual(serviceClock(at("2026-11-01T01:30:00-08:00"), LA), { date: "2026-11-01", secs: 5400 });
  assert.deepEqual(serviceClock(at("2026-11-01T12:00:00-08:00"), LA), { date: "2026-11-01", secs: 43200 });
  assert.deepEqual(serviceClock(at("2026-11-01T23:00:00-08:00"), LA), { date: "2026-11-01", secs: 82800 });
});

test("fall back: the hour before the anchor clamps to the day's start", () => {
  // Seconds cross the boundary unsigned. `previousClock` answers this
  // hour truly.
  assert.deepEqual(serviceClock(at("2026-11-01T00:30:00-07:00"), LA), { date: "2026-11-01", secs: 0 });
});

test("an unknown or absent zone reads the device's", () => {
  const now = at("2026-11-06T15:30:00Z");
  assert.deepEqual(serviceClock(now, "Not/AZone"), serviceClock(now, undefined));
});

test("the previous date's clock: just after midnight it reads past 24:00", () => {
  assert.deepEqual(previousClock(at("2026-11-07T00:30:00-08:00"), LA), { date: "2026-11-06", from_secs: 88200, length_secs: 86400 });
  assert.deepEqual(previousClock(at("2026-11-06T15:30:00-08:00"), LA), { date: "2026-11-05", from_secs: 142200, length_secs: 86400 });
});

test("the previous date's clock across fall back: a 25-hour day, and the hour before the anchor", () => {
  assert.deepEqual(previousClock(at("2026-11-01T00:30:00-07:00"), LA), { date: "2026-10-31", from_secs: 88200, length_secs: 90000 });
});

test("the previous date's clock across spring forward: a 23-hour day", () => {
  assert.deepEqual(previousClock(at("2026-03-08T00:30:00-08:00"), LA), { date: "2026-03-07", from_secs: 88200, length_secs: 82800 });
});
