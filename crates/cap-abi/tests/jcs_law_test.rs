//! The canonical form is RFC 8785. The properties come from the
//! standard. The writer must agree with the `reference` below for any JSON
//! value and for every real document, whatever order its fields are declared
//! in.

use proptest::prelude::*;
use serde_json::{json, Value};

fn jcs(v: &impl serde::Serialize) -> String {
    cap_abi::jcs::to_string(v).unwrap()
}

/// A plain reference writer with no streaming and no buffers.
fn reference(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if n.is_f64() => {
                if f == 0.0 {
                    "0".into()
                } else {
                    ryu_js::Buffer::new().format_finite(f).to_string()
                }
            }
            _ => n.to_string(),
        },
        Value::String(s) => {
            let mut out = String::from("\"");
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\u{8}' => out.push_str("\\b"),
                    '\t' => out.push_str("\\t"),
                    '\n' => out.push_str("\\n"),
                    '\u{c}' => out.push_str("\\f"),
                    '\r' => out.push_str("\\r"),
                    c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                    c => out.push(c),
                }
            }
            out.push('"');
            out
        }
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(reference).collect::<Vec<_>>().join(",")
        ),
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            let entries: Vec<String> = keys
                .iter()
                .map(|k| {
                    format!(
                        "{}:{}",
                        reference(&Value::String((*k).clone())),
                        reference(&map[*k])
                    )
                })
                .collect();
            format!("{{{}}}", entries.join(","))
        }
    }
}

#[test]
fn the_standards_sample_comes_out_byte_for_byte() {
    // RFC 8785, sections 3.2.2 and 3.2.3.
    let input: Value = serde_json::from_str(
        r#"{
          "numbers": [333333333.33333329, 1E30, 4.50, 2e-3, 0.000000000000000000000000001],
          "string": "\u20ac$\u000F\u000aA'\u0042\u0022\u005c\\\"\/",
          "literals": [null, true, false]
        }"#,
    )
    .unwrap();
    assert_eq!(
        jcs(&input),
        "{\"literals\":[null,true,false],\"numbers\":[333333333.3333333,1e+30,4.5,0.002,1e-27],\"string\":\"€$\\u000f\\nA'B\\\"\\\\\\\\\\\"/\"}"
    );
}

#[test]
fn keys_sort_by_utf16_code_units() {
    // RFC 8785, section 3.2.3. An astral character (a surrogate pair) sorts
    // before U+FB33, though its UTF-8 bytes sort after.
    let input: Value = serde_json::from_str(
        r#"{"€":"Euro Sign","\r":"Carriage Return","דּ":"Hebrew Letter Dalet With Dagesh","1":"One","😀":"Emoji: Grinning Face","\u0080":"Control","ö":"Latin Small Letter O With Diaeresis"}"#,
    )
    .unwrap();
    let out: Value = serde_json::from_str(&jcs(&input)).unwrap();
    let text = jcs(&input);
    let order: Vec<usize> = ["\\r", "\"1\"", "\u{80}", "ö", "€", "😀", "\u{fb33}"]
        .iter()
        .map(|k| text.find(k).unwrap())
        .collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{text}");
    assert_eq!(out, input);
}

#[test]
fn numbers_are_written_as_ecmascript_writes_them() {
    for (n, want) in [
        (0.0, "0"),
        (-0.0, "0"),
        (1.0, "1"),
        (-1.5, "-1.5"),
        (1e21, "1e+21"),
        (1e20, "100000000000000000000"),
        (1e-7, "1e-7"),
        (0.000001, "0.000001"),
        (45.6934, "45.6934"),
        (-122.59998999999999, "-122.59998999999999"),
        (5e-324, "5e-324"),
        (1.7976931348623157e308, "1.7976931348623157e+308"),
    ] {
        assert_eq!(jcs(&n), want, "{n:e}");
    }
    assert_eq!(
        jcs(&u64::MAX),
        "18446744073709551615",
        "an integer type is written whole"
    );
    assert!(cap_abi::jcs::to_string(&f64::NAN).is_err());
    assert!(cap_abi::jcs::to_string(&vec![f64::INFINITY]).is_err());
}

#[test]
fn control_characters_use_the_short_escapes_else_lower_case_hex() {
    assert_eq!(
        jcs(&"\u{8}\t\n\u{c}\r\u{1}\u{1f}\u{7f}\u{2028}/"),
        "\"\\b\\t\\n\\f\\r\\u0001\\u001f\u{7f}\u{2028}/\""
    );
}

fn arb_json() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(|n| json!(n)),
        any::<f64>()
            .prop_filter("finite", |f| f.is_finite())
            .prop_map(|f| json!(f)),
        "\\PC{0,6}".prop_map(Value::String),
        "[\\x00-\\x1f\"\\\\a-z\u{80}\u{fb33}\u{1f600}]{0,4}".prop_map(Value::String),
    ];
    leaf.prop_recursive(4, 64, 8, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..6).prop_map(Value::Array),
            proptest::collection::btree_map("[a-z\u{e9}\u{fb33}\u{1f600}0-9_]{0,4}", inner, 0..8)
                .prop_map(|m| Value::Object(m.into_iter().collect())),
        ]
    })
}

proptest! {
    #[test]
    fn any_json_value_comes_out_as_the_reference_writes_it(v in arb_json()) {
        let text = jcs(&v);
        prop_assert_eq!(&text, &reference(&v));
        prop_assert_eq!(reference(&serde_json::from_str::<Value>(&text).unwrap()), text);
    }
}

fn agrees_typed<T: serde::Serialize>(what: &str, doc: &T) {
    let value = serde_json::to_value(doc).unwrap();
    let text = jcs(doc);
    assert_eq!(text, reference(&value), "{what}");
    assert_eq!(
        cap_abi::canonical_hash(doc).unwrap(),
        cap_abi::content_hash(text.as_bytes()),
        "{what}: the hash is of exactly these bytes"
    );
}

#[test]
fn every_real_document_comes_out_as_the_reference_writes_it() {
    let zip = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/e2e/fixtures/sample-gtfs.zip"
    ))
    .unwrap();
    let feed = transit_codec::Feed::from_zip_bytes(&zip).unwrap();
    let imported = transit_codec::import(&feed);
    agrees_typed("network", &imported.network);
    agrees_typed("boundary", &imported.boundary);

    // Fields that this build does not know, out of order.
    let mut newer = serde_json::to_value(&imported.network).unwrap();
    newer["aaa_first"] = json!(1);
    newer["zzz_last"] = json!({"b": 2, "a": 1});
    let newer: transit_network::Network = serde_json::from_value(newer).unwrap();
    agrees_typed("network with unknown fields", &newer);

    let edits: transit_sync::EditDocument = serde_json::from_value(json!({"kind": "edit_records", "schema_version": 2, "records": [
        {"id": "e1", "rev": 1, "status": "accepted", "updated_at": 1, "diversion": {"id": "d1", "route_id": "10", "start_date": "2026-10-23", "end_date": "2026-10-23",
            "first_kept_stop": "A", "last_kept_stop": "B", "skipped_stops": ["C"], "added_secs": 300, "detour": [{"lat": 45.6, "lon": -122.6}]}},
        {"id": "e2", "rev": 1, "status": "accepted", "updated_at": 2, "cancel": {"id": "c1", "trip_id": "t1", "date": "2026-10-23"}}
    ]})).unwrap();
    agrees_typed("edit document", &edits);

    let stops: transit_stops::Stops = serde_json::from_value(json!({"kind": "stops", "schema_version": 1, "stops": {
        "6124": [{"on": "2004-09-01", "event": "in_service", "lat": 45.6934, "lon": -122.6512, "name": "NE 99th St & Hwy 99", "wheelchair": true,
            "amenities": {"shelter": true}, "attachments": [{"hash": "sha256:00", "type": "image/jpeg"}], "a_newer_field": 1}]}}))
    .unwrap();
    agrees_typed("stops", &stops);
}

#[test]
fn a_large_object_out_of_order_comes_out_sorted() {
    // More keys than the writer serializes one pass per key, so it buffers.
    struct Map<'a>(&'a [(String, u32)]);
    impl serde::Serialize for Map<'_> {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeMap;
            let mut m = s.serialize_map(Some(self.0.len()))?;
            for (k, v) in self.0 {
                m.serialize_entry(k, v)?;
            }
            m.end()
        }
    }
    let entries: Vec<(String, u32)> = (0..200u32).rev().map(|i| (format!("k{i:03}"), i)).collect();
    let text = jcs(&Map(&entries));
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(text, reference(&value));
}

fn arrival_order(text: &str) -> Vec<String> {
    struct Order(Vec<String>);
    impl<'de> serde::Deserialize<'de> for Order {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct V;
            impl<'de> serde::de::Visitor<'de> for V {
                type Value = Order;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("an object")
                }
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    mut m: A,
                ) -> Result<Order, A::Error> {
                    let mut keys = vec![];
                    while let Some(k) = m.next_key::<String>()? {
                        keys.push(k);
                        m.next_value::<serde::de::IgnoredAny>()?;
                    }
                    Ok(Order(keys))
                }
            }
            d.deserialize_map(V)
        }
    }
    serde_json::from_str::<Order>(text).unwrap().0
}

/// A network holds records by the hundred thousand. Declared in canonical
/// order, their bytes stream in one pass with no reordering.
#[test]
fn the_networks_many_records_arrive_in_canonical_order() {
    let zip = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/e2e/fixtures/sample-gtfs.zip"
    ))
    .unwrap();
    let network =
        transit_codec::import(&transit_codec::Feed::from_zip_bytes(&zip).unwrap()).network;
    let in_order = |what: &str, text: String| {
        let keys = arrival_order(&text);
        assert!(
            keys.windows(2)
                .all(|w| cap_abi::jcs::key_order(&w[0], &w[1]).is_lt()),
            "{what}: {keys:?}"
        );
    };
    let full = transit_network::Trip {
        bikes: Some(true),
        cars: Some(false),
        headsign: Some("h".into()),
        short_name: Some("s".into()),
        wheelchair: Some(true),
        ..Default::default()
    };
    in_order(
        "a trip with every field",
        serde_json::to_string(&full).unwrap(),
    );
    let offset = transit_network::StopOffset {
        arrival_secs: Some(1),
        departure_secs: Some(2),
    };
    in_order("a stop's times", serde_json::to_string(&offset).unwrap());
    let point = transit_network::AlignmentPoint {
        lat: 1.0,
        lon: 2.0,
        distance: Some(3.0),
    };
    in_order("an alignment point", serde_json::to_string(&point).unwrap());
    for pt in network.pattern_trips.values() {
        in_order("pattern trips", serde_json::to_string(pt).unwrap());
    }
}
