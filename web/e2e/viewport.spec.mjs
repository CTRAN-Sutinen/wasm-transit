// Where a map's camera starts and when it frames the whole network
// (src/viewport.ts) — pure rules, no browser. They exist so a map
// opens on the view the rider will end up on: every camera move before
// that one fetches tiles that are thrown away.
//
//   1. The start view: a rider's last known fix, when tiles cover it;
//      else the view the network was last framed on; else the
//      archive's own center; else the world.
//   2. Framing the whole network is the answer only when nothing
//      better is known — not while a fix is pending, not when the
//      camera already sits inside the network.

import { test, expect } from "@playwright/test";
import { RIDER_ZOOM, archiveView, boundsOf, sameView, shouldFrameNetwork, startView, within } from "../src/viewport.ts";

// A metro-sized archive (Portland–Vancouver), as `just basemap` extracts.
const metro = { minLon: -123.2, minLat: 45.25, maxLon: -122.1, maxLat: 45.95 };
const downtown = { lat: 45.6266, lon: -122.6746 };
const elsewhere = { lat: 48.4, lon: -123.4 }; // outside the archive

test("the archive's view is its center, zoomed to its width", () => {
  const v = archiveView(metro);
  expect(v.center[0]).toBeCloseTo(-122.65, 6);
  expect(v.center[1]).toBeCloseTo(45.6, 6);
  expect(v.zoom).toBe(Math.round(Math.log2(360 / 1.1)) + 1);
  // A sliver of an archive never zooms past what the formula floors.
  expect(archiveView({ minLon: 0, minLat: 0, maxLon: 0, maxLat: 0 }).zoom).toBeGreaterThanOrEqual(2);
});

test("a fix inside the archive is where the map starts, at the rider's zoom", () => {
  expect(startView(metro, downtown)).toEqual({ center: [downtown.lon, downtown.lat], zoom: RIDER_ZOOM });
});

test("a fix tiles do not cover is not a start: the archive's own view", () => {
  expect(startView(metro, elsewhere)).toEqual(archiveView(metro));
});

test("no fix: the archive's view", () => {
  expect(startView(metro, null)).toEqual(archiveView(metro));
});

test.describe("the view the network was last framed on", () => {
  const framed = { center: [-122.85, 45.7], zoom: 8.7 };

  test("is the start when there is no fix", () => {
    expect(startView(metro, null, framed)).toEqual(framed);
  });

  test("yields to a fix the tiles cover", () => {
    expect(startView(metro, downtown, framed)).toEqual({ center: [downtown.lon, downtown.lat], zoom: RIDER_ZOOM });
  });

  test("is the start when the fix is not covered either", () => {
    expect(startView(metro, elsewhere, framed)).toEqual(framed);
  });

  test("is not the start where tiles do not cover it: the archive's view", () => {
    expect(startView(metro, null, { center: [-73.5, 45.5], zoom: 12 })).toEqual(archiveView(metro));
  });

  test("with no archive the world is covered: it is the start", () => {
    expect(startView(null, null, framed)).toEqual(framed);
  });
});

test("with no archive the world is covered: a fix anywhere is the start", () => {
  expect(startView(null, elsewhere)).toEqual({ center: [elsewhere.lon, elsewhere.lat], zoom: RIDER_ZOOM });
  expect(startView(null, null)).toEqual({ center: [0, 0], zoom: 1 });
});

test("within holds on the edges and nowhere else", () => {
  expect(within(metro, { lat: 45.25, lon: -123.2 })).toBe(true);
  expect(within(metro, { lat: 45.95, lon: -122.1 })).toBe(true);
  expect(within(metro, { lat: 45.96, lon: -122.6 })).toBe(false);
  expect(within(metro, { lat: 45.6, lon: -122.0 })).toBe(false);
});

test("the bounds of points are their extremes; none for no points", () => {
  expect(boundsOf([])).toBeNull();
  expect(
    boundsOf([
      { lat: 1, lon: 5 },
      { lat: -2, lon: 3 },
      { lat: 4, lon: 4 },
    ]),
  ).toEqual({ minLon: 3, minLat: -2, maxLon: 5, maxLat: 4 });
});

test("two views are the same when no pixel would move", () => {
  const a = { center: [-122.85, 45.7], zoom: 8.7 };
  expect(sameView(a, { ...a })).toBe(true);
  // A hundredth of a pixel at this zoom.
  expect(sameView(a, { center: [-122.85 + 0.00001, 45.7], zoom: 8.7 })).toBe(true);
  // A whole screen away, or a zoom step apart, is a different view.
  expect(sameView(a, { center: [-122.5, 45.7], zoom: 8.7 })).toBe(false);
  expect(sameView(a, { center: a.center, zoom: 9.7 })).toBe(false);
});

test.describe("framing the network", () => {
  const network = boundsOf([
    { lat: 45.5, lon: -122.8 },
    { lat: 45.7, lon: -122.5 },
  ]);
  const none = { stops: network, start: null, here: null, selected: false, locating: false };

  test("nothing better known: frame it", () => {
    expect(shouldFrameNetwork(none)).toBe(true);
  });

  test("not before there are stops to frame", () => {
    expect(shouldFrameNetwork({ ...none, stops: null })).toBe(false);
  });

  test("not while a fix is pending: the camera will go to the rider", () => {
    expect(shouldFrameNetwork({ ...none, locating: true })).toBe(false);
  });

  test("not when the rider is somewhere (a fix, a picked stop): the camera went there", () => {
    expect(shouldFrameNetwork({ ...none, here: downtown })).toBe(false);
    expect(shouldFrameNetwork({ ...none, selected: true })).toBe(false);
  });

  test("not when the camera started inside the network", () => {
    expect(shouldFrameNetwork({ ...none, start: downtown })).toBe(false);
  });

  test("when the camera started outside the network (a stale fix), frame it", () => {
    expect(shouldFrameNetwork({ ...none, start: elsewhere })).toBe(true);
  });
});
