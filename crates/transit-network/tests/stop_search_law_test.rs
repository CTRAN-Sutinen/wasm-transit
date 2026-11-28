//! Stop search (thresholds D-91; cap-directory/OPS.md, `stops`).
//! "vancouver
//! mall" finds the stops whose sign says "Van Mall". There are no authored
//! aliases.
//!
//! 1. Every term that is a substring of the name or id finds the stop as an
//!    exact match.
//! 2. A name word of three letters or more answers any term it begins. A name
//!    word answers a term it is a subsequence of when they share the first
//!    and last letter ("Blvd" answers "boulevard").
//! 3. A term within one edit of a name word, or of its start, finds it. A term
//!    under four letters is never corrected.
//! 4. Exact matches list before abbreviations, and abbreviations before typos.
//!    Each tier sorts by name, then id.
//! 5. A term that no word answers finds nothing.

use std::collections::BTreeMap;

use proptest::prelude::*;
use transit_network::listing::{find_stops, name_match, NameMatch};
use transit_network::{Network, Stop};

fn network(names: &[String]) -> Network {
    let stops: BTreeMap<String, Stop> = names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            (
                format!("{i}"),
                Stop {
                    name: n.clone(),
                    lat: 45.5,
                    lon: -122.5,
                    ..Default::default()
                },
            )
        })
        .collect();
    Network {
        name: "Stop search laws".into(),
        stops,
        ..Default::default()
    }
}

fn word() -> impl Strategy<Value = String> {
    "[a-z]{3,10}"
}

fn name() -> impl Strategy<Value = String> {
    prop::collection::vec(word(), 1..4).prop_map(|w| {
        w.into_iter()
            .map(|w| {
                let mut c = w.chars();
                c.next()
                    .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" ")
    })
}

fn typo(w: &str, at: usize, kind: u8, letter: char) -> String {
    let mut c: Vec<char> = w.chars().collect();
    let i = at % c.len();
    match kind % 4 {
        0 => c[i] = letter,
        1 => c.insert(i, letter),
        2 => {
            c.remove(i);
        }
        _ => {
            if i + 1 < c.len() {
                c.swap(i, i + 1);
            }
        }
    }
    c.into_iter().collect()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn law1_exact_still_finds(n in name(), start in 0usize..20, len in 1usize..6) {
        let lower = n.to_lowercase();
        let s = start % lower.len();
        let e = (s + len).min(lower.len());
        let term = lower[s..e].trim().to_string();
        prop_assume!(!term.is_empty() && !term.contains(' '));
        prop_assert_eq!(name_match(&term, &n, "id"), Some(NameMatch::Exact));
    }

    #[test]
    fn law2_an_abbreviation_answers_the_full_word(w in word(), rest in "[a-z]{1,6}") {
        let sign = format!("{} Mall", &w[..3]);
        let typed = format!("{}{} mall", &w[..3], rest);
        prop_assert!(name_match(&typed, &sign, "id").is_some(), "{sign:?} answers {typed:?}");
    }

    #[test]
    fn law3_a_typo_still_finds(w in "[a-z]{4,12}", at in 0usize..12, kind in 0u8..4, letter in proptest::char::range('a', 'z')) {
        let typed = typo(&w, at, kind, letter);
        prop_assume!(typed.chars().count() >= 4);
        let sign = format!("Stop {w}");
        prop_assert!(name_match(&typed, &sign, "id").is_some(), "{sign:?} answers {typed:?}");
        let long = format!("Stop {w}ington");
        prop_assert!(name_match(&typed, &long, "id").is_some(), "{long:?} answers {typed:?}");
    }

    #[test]
    fn law4_exact_first_then_by_name(names in prop::collection::vec(name(), 1..12), q in word()) {
        let net = network(&names);
        let found = find_stops(&net, &q);
        let tiers: Vec<(NameMatch, String, String)> = found
            .iter()
            .map(|id| {
                let s = &net.stops[id];
                (name_match(&q, &s.name, id).expect("found means matched"), s.name.clone(), id.clone())
            })
            .collect();
        let mut sorted = tiers.clone();
        sorted.sort();
        prop_assert_eq!(&tiers, &sorted, "tier, then name, then id");
        for (id, s) in &net.stops {
            prop_assert_eq!(found.contains(id), name_match(&q, &s.name, id).is_some());
        }
        let all = find_stops(&net, "");
        let mut by_name: Vec<(String, String)> = net.stops.iter().map(|(id, s)| (s.name.clone(), id.clone())).collect();
        by_name.sort();
        prop_assert_eq!(all, by_name.into_iter().map(|(_, id)| id).collect::<Vec<_>>());
    }
}

#[test]
fn the_everyday_name_finds_the_sign() {
    for sign in ["Van Mall Transit Center", "Vancouver Mall"] {
        assert!(name_match("vancouver mall", sign, "1").is_some(), "{sign}");
    }
    assert_eq!(
        name_match("van mall", "Van Mall Transit Center", "1"),
        Some(NameMatch::Exact)
    );
    assert_eq!(
        name_match("vancouver mall", "Van Mall Transit Center", "1"),
        Some(NameMatch::Abbreviation)
    );
    assert_eq!(
        name_match("main street", "Main St & 5th", "1"),
        Some(NameMatch::Abbreviation)
    );
    assert_eq!(
        name_match("forth plain", "Fourth Plain & Andresen", "1"),
        Some(NameMatch::Typo)
    );
    assert_eq!(
        name_match("andersen", "Fourth Plain & Andresen", "1"),
        Some(NameMatch::Typo)
    );
}

#[test]
fn law5_every_term_must_match() {
    assert_eq!(name_match("vancouver zebra", "Van Mall", "1"), None);
    assert_eq!(
        name_match("xyz", "Van Mall", "1"),
        None,
        "under four letters: never corrected"
    );
    assert_eq!(name_match("newport", "NE 4th St", "1"), None);
}
