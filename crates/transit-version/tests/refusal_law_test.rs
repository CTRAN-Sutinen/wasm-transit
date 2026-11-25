//! Pins the coded refusals that hosts act on (docs/architecture/bus.md,
//! "Answers").

use serde_json::json;
use transit_version::{refused, write_refused, Refusal, RefusalCode as Code};

/// 1. A document two versions ahead names its kind and both versions, and was
///    not read.
#[test]
fn two_versions_ahead_is_newer_and_unread() {
    let r = refused("network", 4, 2);
    assert_eq!(r.code, Some(Code::NewerVersion { kind: "network".into(), found: 4, reads: 2, read: false }));
}

/// 2. A document one version ahead that the build cannot read is refused the
///    same way.
#[test]
fn one_version_ahead_unreadable_is_newer_and_unread() {
    let r = refused("watchlist", 3, 2);
    assert_eq!(r.code, Some(Code::NewerVersion { kind: "watchlist".into(), found: 3, reads: 2, read: false }));
}

/// 3. A write over a document read one version ahead is refused as read.
#[test]
fn a_write_over_a_read_ahead_copy_is_newer_and_read() {
    let r = write_refused("network", 3, 2);
    assert_eq!(r.code, Some(Code::NewerVersion { kind: "network".into(), found: 3, reads: 2, read: true }));
}

/// 4. An older document asks for a migration, which no host acts on, so it
///    has no code.
#[test]
fn an_older_document_has_no_code() {
    assert_eq!(refused("network", 1, 2).code, None);
}

/// 5. Every error is an object with its message. A coded one adds its code
///    and fields beside the message.
#[test]
fn the_shape_is_a_message_and_an_optional_code() {
    let plain = Refusal::from("invalid request".to_string());
    assert_eq!(serde_json::to_value(&plain).unwrap(), json!({"message": "invalid request"}));
    let coded = refused("network", 4, 2);
    assert_eq!(
        serde_json::to_value(&coded).unwrap(),
        json!({"message": coded.message, "code": "newer_version", "kind": "network", "found": 4, "reads": 2, "read": false})
    );
    let unknown = Refusal::unknown_document("network", "sha256:ab", "load_network");
    assert_eq!(
        serde_json::to_value(&unknown).unwrap(),
        json!({"message": unknown.message, "code": "unknown_document", "kind": "network", "hash": "sha256:ab", "load": "load_network"})
    );
}

/// 6. A prefix that places the refusal keeps its code.
#[test]
fn context_keeps_the_code() {
    let r = refused("edit_records", 3, 2).context("record r1");
    assert!(r.message.starts_with("record r1: "));
    assert!(matches!(r.code, Some(Code::NewerVersion { found: 3, .. })));
}
