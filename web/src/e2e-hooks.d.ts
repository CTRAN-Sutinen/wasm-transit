// E2E/inspection hooks the pages set on `window`. Tests read them;
// no page code reads them back.
interface Window {
  /// The rider map's MapLibre instance (RideMap): e2e specs project
  /// points through it to tap the map.
  __map: unknown;
}
