//! The bus profile (D-26): which streets a planner may draw a bus along.
//! Every class is allowed. A street's class is a grey area the planner
//! judges, so nothing is refused. The classes that the data does not
//! clearly allow a bus on come back named. They are `unknown` (the layer
//! gave no class), and the path-like `path`, `steps` and `track`. The
//! planner confirms these streets ([`crate::StreetNetwork::route_along`]
//! names every street a path follows; the profile picks these out).
//! The list is the op's parameter; this is its default.

use crate::StreetClass;

/// The classes the bus profile asks the planner to confirm.
pub const CONFIRM: [StreetClass; 4] = [StreetClass::Unknown, StreetClass::Path, StreetClass::Steps, StreetClass::Track];

/// Whether the bus profile asks the planner to confirm a street of
/// this class.
pub fn confirms(class: StreetClass) -> bool {
    CONFIRM.contains(&class)
}
