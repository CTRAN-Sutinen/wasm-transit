// E2E/inspection hooks the pages set on `window`. Tests read them;
// no page code reads them back.
interface Window {
  /// The rider map's MapLibre instance (RideMap): e2e specs project
  /// points through it to tap the map.
  __map: unknown;
  /// The engine's inspection (src/engine.ts): what a module holds, its
  /// linear memory and resets, the memory ceiling, and a replay of the
  /// last op asked of a capability past the answer cache.
  __bus: import("./engine").Inspection;
}
