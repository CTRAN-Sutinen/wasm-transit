/// The words a stop's life is shown in (D-157): its statuses, its
/// events, why a change was refused, and the amenity names (D-154).
/// FORMATTING only — the statuses, what a stop may become, and every
/// refusal are cap-stops' answers.

export const STATUS_WORDS: Record<string, string> = {
  planned: "Planned",
  permitted: "Permitted",
  built: "Built",
  in_service: "In service",
  suspended: "Suspended",
  retired: "Retired",
};

/// Each status's colour on the map and its key (fill, outline).
export const STATUS_COLORS: Record<string, [string, string]> = {
  planned: ["#ede9fe", "#7c3aed"],
  permitted: ["#dbeafe", "#2563eb"],
  built: ["#ccfbf1", "#0d9488"],
  in_service: ["#ffffff", "#334155"],
  suspended: ["#fecaca", "#b91c1c"],
  retired: ["#e2e8f0", "#94a3b8"],
};

export const EVENT_WORDS: Record<string, string> = {
  planned: "Planned",
  permitted: "Permitted",
  built: "Built",
  in_service: "In service",
  suspended: "Suspended",
  moved: "Moved",
  changed: "Changed",
  retired: "Retired",
};

/// The button that records each lifecycle event.
export const ACTION_WORDS: Record<string, string> = {
  planned: "Planned",
  permitted: "Permitted",
  built: "Built",
  in_service: "In service",
  suspended: "Suspend",
  retired: "Retire",
};

const PROBLEMS: Record<string, string> = {
  no_events: "the stop would have no events left",
  unknown_event: "the event type is not known",
  bad_date: "that is not a date",
  no_date: "the event needs a date",
  out_of_order: "it would be dated before the event above it",
  first_event: "a stop's history must begin with a stage: planned, permitted, built, or in service",
  after_retired: "the stop is retired by then",
  stage_back: "the stop is already at or past that stage then",
  not_in_service: "the stop is not in service then",
  already_in_service: "the stop is already in service then",
  until_not_after: "the end must come after the first day",
  misplaced: "the event carries something its type does not set",
  no_point: "the stop needs a whole location",
  bad_point: "the location is not on the earth",
  unknown_stop: "it names a stop that is not recorded",
  replaces_itself: "a stop cannot replace itself",
};

export function problemWords(kind: string): string {
  return PROBLEMS[kind] ?? kind.replaceAll("_", " ");
}

/// The core amenity names (D-154, transit-stops `AMENITIES`) as a
/// person reads them.
export const AMENITIES: [string, string][] = [
  ["shelter", "Shelter"],
  ["bench", "Bench"],
  ["bin", "Garbage can"],
  ["lit", "Lighting"],
  ["departures_board", "Printed schedule"],
  ["passenger_information_display", "Real-time sign"],
  ["bicycle_parking", "Bike rack"],
  ["boarding_pad", "Boarding pad"],
  ["tactile_paving", "Tactile paving"],
];

export function amenityWords(name: string): string {
  return AMENITIES.find(([k]) => k === name)?.[1] ?? name.replaceAll("_", " ");
}
