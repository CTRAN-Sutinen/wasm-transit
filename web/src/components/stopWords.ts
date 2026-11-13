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
  bad_answer: "a survey answer is not one the question takes",
};

export function problemWords(kind: string): string {
  return PROBLEMS[kind] ?? kind.replaceAll("_", " ");
}

/// The amenity names (D-154; the list a page offers is cap-stops
/// `questions`') as a person reads them.
const AMENITY_WORDS: Record<string, string> = {
  shelter: "Shelter",
  bench: "Bench",
  bin: "Garbage can",
  lit: "Lighting",
  departures_board: "Printed schedule",
  passenger_information_display: "Real-time sign",
  bicycle_parking: "Bike rack",
  boarding_pad: "Front door pad",
  tactile_paving: "Tactile paving",
  rear_boarding_pad: "Rear door pad",
  seat: "Seat (not a bench)",
  post: "Sign post",
};

export function amenityWords(name: string): string {
  return AMENITY_WORDS[name] ?? plain(name);
}

/// A name or an answer with no words of its own, readable.
function plain(name: string): string {
  const t = name.replaceAll(":", " ").replaceAll("_", " ");
  return t.charAt(0).toUpperCase() + t.slice(1);
}

/// The site questions (D-158; the questions and their answers are
/// cap-stops `questions`') as the worksheet words them, grouped as it
/// groups them. A question the library adds and no group names shows
/// under "Other".
const SITE_WORDS: Record<string, string> = {
  on_street: "On street",
  cross_street: "Cross street",
  city: "City",
  county: "County",
  lanes: "Traffic lanes",
  speed_limit_mph: "Speed limit (mph)",
  direction: "Route direction",
  side: "Side of street",
  position: "Near or far side",
  land_use: "Land use",
  shoulder: "Shoulder",
  surface: "Stop surface",
  sidewalk: "Sidewalk",
  "sidewalk:surface": "Sidewalk surface",
  "sidewalk:connection": "Sidewalk connection",
  ada: "ADA",
  curb: "Curb",
  curb_cut: "Curb cut",
  street_light: "Street light",
  turnout: "Turnout",
  crossing: "Pedestrian crossing",
  advertising: "Advertising",
  "light:source": "Light source",
  "light:type": "Light type",
  "post:type": "Post type",
  "sign:type": "Sign type",
  "shelter:type": "Shelter type",
  "kiosk:type": "Schedule kiosk type",
  "bin:type": "Garbage can type",
  "seat:type": "Seating type",
  landmark: "Landmark",
  remarks: "Remarks",
};

export const SITE_GROUPS: { title: string; keys: string[] }[] = [
  { title: "Location", keys: ["on_street", "cross_street", "city", "county", "direction", "side", "position", "land_use"] },
  { title: "Street", keys: ["lanes", "speed_limit_mph", "shoulder", "turnout", "crossing", "street_light"] },
  { title: "Surface and access", keys: ["surface", "sidewalk", "sidewalk:surface", "sidewalk:connection", "ada", "curb", "curb_cut"] },
  { title: "Fixtures", keys: ["post:type", "sign:type", "shelter:type", "kiosk:type", "seat:type", "bin:type", "light:source", "light:type", "advertising"] },
  { title: "Notes", keys: ["landmark", "remarks"] },
];

export function siteWords(key: string): string {
  return SITE_WORDS[key] ?? plain(key);
}

const ANSWER_WORDS: Record<string, string> = {
  counterclockwise: "Counter clockwise",
  one_direction: "1 direction",
  within_200ft: "Within 200 ft of the stop",
  within_50ft: "Within 50 ft",
  at_stop: "At the stop",
  other_source: "Other source",
  driveway_close: "Driveway cut close",
  right_turn_only: "Right turn only",
  mid_block: "Mid-block",
  curb_mark: "Mark on curb",
  ada: "ADA",
};

export function answerWords(value: string): string {
  return ANSWER_WORDS[value] ?? plain(value);
}

/// The other systems a stop or its parts are named in (D-160), as the
/// worksheets name them; the agency may add its own.
export const REFS: [string, string][] = [
  ["sign", "Sign ID"],
  ["tms", "TMS"],
  ["hp", "HP"],
  ["gis", "GIS"],
  ["shelter", "Shelter ID"],
];

export function refWords(name: string): string {
  return REFS.find(([k]) => k === name)?.[1] ?? plain(name);
}

/// Why a request was refused (cap-requests' refusals, D-159), in words.
export function requestProblemWords(p: { kind: string; field?: string; value?: string; date?: string }): string {
  switch (p.kind) {
    case "no_id":
      return "the request needs an id";
    case "no_stop":
      return "the request names no stop";
    case "unknown":
      return `${(p.field ?? "").replaceAll("_", " ")} "${p.value}" is not one this app knows`;
    case "bad_date":
      return `"${p.date}" is not a date`;
    case "negative_offset":
      return "the distance from the cross street cannot be below zero";
    case "no_work_order":
      return "a request whose work order is open needs the work order's id";
    case "no_completion":
      return "a completed request needs the date it was completed";
    case "completed_before_requested":
      return "it cannot be completed before it was requested";
    default:
      return p.kind.replaceAll("_", " ");
  }
}
