//! The bus profile allows every class, because the planner judges a
//! street's class. It only names the classes that the data does not clearly
//! allow a bus on, for the planner to confirm.

use crate::StreetClass;

pub const CONFIRM: [StreetClass; 4] = [StreetClass::Unknown, StreetClass::Path, StreetClass::Steps, StreetClass::Track];

pub fn confirms(class: StreetClass) -> bool {
    CONFIRM.contains(&class)
}
