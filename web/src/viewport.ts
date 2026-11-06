// Where a map's camera starts and when it frames the whole network —
// presentation rules, pure. They answer one question: what view will
// the rider end up on? Every camera move before that view fetches
// tiles that are thrown away, so a map opens on it when it is known
// (the rider's last fix) and does not visit another on the way.
// Nothing here is data the bus holds; no op reads it.

export type Point = { lat: number; lon: number };
export type Bounds = { minLon: number; minLat: number; maxLon: number; maxLat: number };
/// MapLibre's camera: [lon, lat] and a zoom.
export type View = { center: [number, number]; zoom: number };

/// The zoom the map takes on the rider, a stop, or a spot — the
/// street level the nearby list answers for.
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

/// The view over a whole archive: its center, zoomed to its width.
/// Tiles exist only inside it, so a regional extract starts there.
export function archiveView(b: Bounds): View {
  return {
    center: [(b.minLon + b.maxLon) / 2, (b.minLat + b.maxLat) / 2],
    zoom: Math.max(2, Math.round(Math.log2(360 / Math.max(0.05, b.maxLon - b.minLon))) + 1),
  };
}

/// Where a map opens. `archive` is the local tile archive's bounds,
/// or null when the tiles are the world's (OpenFreeMap); `near` is
/// the rider's last known fix, if any; `framed` is the view the
/// network was last framed on, if the device remembers one. In that
/// order, the first that tiles cover is the start (a view outside
/// the archive would open on blank); else the archive's own view;
/// else the world.
export function startView(archive: Bounds | null, near: Point | null, framed: View | null = null): View {
  const covered = (p: Point) => !archive || within(archive, p);
  if (near && covered(near)) return { center: [near.lon, near.lat], zoom: RIDER_ZOOM };
  if (framed && covered({ lon: framed.center[0], lat: framed.center[1] })) return framed;
  return archive ? archiveView(archive) : { center: [0, 0], zoom: 1 };
}

/// Two views are the same when no pixel would move: the centers
/// within half a pixel at that zoom (512-px tiles) and the zooms
/// equal to a hundredth. A camera already on a view needs no move to
/// it — and a move that changes nothing still repaints and refetches.
export function sameView(a: View, b: View): boolean {
  const degPerPx = 360 / (512 * 2 ** a.zoom);
  const lat = Math.cos((a.center[1] * Math.PI) / 180);
  return (
    Math.abs(a.zoom - b.zoom) < 0.01 &&
    Math.abs(a.center[0] - b.center[0]) < degPerPx / 2 &&
    Math.abs(a.center[1] - b.center[1]) < (degPerPx * lat) / 2
  );
}

/// Whether the map frames the whole network now. It is the answer
/// only when nothing better is known: there are stops to frame; no
/// fix is pending (the camera will go to the rider); the rider is not
/// somewhere already (a fix, a picked stop); and the camera did not
/// start inside the network (a last fix that is still where the rider
/// is — one outside it is stale, and the network is the useful view).
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
