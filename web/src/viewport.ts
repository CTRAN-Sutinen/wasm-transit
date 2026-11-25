// Every camera move before the final view fetches tiles that are thrown
// away. A map therefore opens on the view when it is known (the rider's
// last fix) and does not visit another on the way.

export type Point = { lat: number; lon: number };
export type Bounds = { minLon: number; minLat: number; maxLon: number; maxLat: number };
export type View = { center: [number, number]; zoom: number };

export const RIDER_ZOOM = 15.5;

export function within(b: Bounds, p: Point): boolean {
  return p.lon >= b.minLon && p.lon <= b.maxLon && p.lat >= b.minLat && p.lat <= b.maxLat;
}

export function boundsOf(points: Point[]): Bounds | null {
  if (points.length === 0) return null;
  const b = { minLon: points[0].lon, minLat: points[0].lat, maxLon: points[0].lon, maxLat: points[0].lat };
  for (const p of points) {
    b.minLon = Math.min(b.minLon, p.lon);
    b.minLat = Math.min(b.minLat, p.lat);
    b.maxLon = Math.max(b.maxLon, p.lon);
    b.maxLat = Math.max(b.maxLat, p.lat);
  }
  return b;
}

/// Tiles exist only inside the archive, so a regional extract starts there.
export function archiveView(b: Bounds): View {
  return {
    center: [(b.minLon + b.maxLon) / 2, (b.minLat + b.maxLat) / 2],
    zoom: Math.max(2, Math.round(Math.log2(360 / Math.max(0.05, b.maxLon - b.minLon))) + 1),
  };
}

/// `archive` is null when the tiles are the world's (OpenFreeMap). The
/// first of `near` and `framed` that the tiles cover is the start,
/// because a view outside the archive opens on blank. Otherwise the map
/// opens on the archive's view, or on the world.
export function startView(archive: Bounds | null, near: Point | null, framed: View | null = null): View {
  const covered = (p: Point) => !archive || within(archive, p);
  if (near && covered(near)) return { center: [near.lon, near.lat], zoom: RIDER_ZOOM };
  if (framed && covered({ lon: framed.center[0], lat: framed.center[1] })) return framed;
  return archive ? archiveView(archive) : { center: [0, 0], zoom: 1 };
}

/// Two views are the same when no pixel would move: centers within half
/// a pixel at that zoom (512-px tiles), zooms within a hundredth. A move
/// that changes nothing still repaints and refetches.
export function sameView(a: View, b: View): boolean {
  const degPerPx = 360 / (512 * 2 ** a.zoom);
  const lat = Math.cos((a.center[1] * Math.PI) / 180);
  return (
    Math.abs(a.zoom - b.zoom) < 0.01 &&
    Math.abs(a.center[0] - b.center[0]) < degPerPx / 2 &&
    Math.abs(a.center[1] - b.center[1]) < (degPerPx * lat) / 2
  );
}

/// A last fix outside the network is stale, so the network is then the
/// useful view. A pending fix or a known rider position wins over it.
export function shouldFrameNetwork(a: {
  stops: Bounds | null;
  start: Point | null;
  here: Point | null;
  selected: boolean;
  locating: boolean;
}): boolean {
  if (!a.stops || a.locating || a.here || a.selected) return false;
  return !(a.start && within(a.stops, a.start));
}
