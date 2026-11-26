//! A new capability and a new kind change no other file: the test
//! capability and the test kind live only in the fixtures
//! (`fixtures::new_names`), and each part that reads the declarations takes
//! them as it takes the real ones.

#[path = "../../cap-abi/src/bin/common/gates.rs"]
#[allow(dead_code)]
mod gates;

use fixtures::new_names::{declared, CAP, KIND};
use std::path::Path;

fn with_test_kind() -> Vec<cap_abi::kinds::Declaration> {
    cap_abi::kinds::KINDS.iter().copied().chain([KIND]).collect()
}

#[test]
fn a_new_kind_and_capability_pass_with_no_other_file_changed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let kinds = with_test_kind();

    assert_eq!(cap_abi::kinds::problems(&kinds), Vec::<String>::new(), "the kind list takes the test kind");

    let table = generator::kind_table::kinds_of(&kinds);
    let entry = table.iter().find(|e| e.store_kind == "test_kind").expect("the kind table has the test kind");
    assert_eq!((entry.receive.as_deref(), entry.module.as_deref()), (Some("testcap"), Some("testcap")));
    assert_eq!(entry.post.as_ref().map(|p| p.path.as_str()), Some("/test-kind"));

    let own: Vec<String> = serde_json::from_slice(&std::fs::read(root.join("server/capabilities.json")).unwrap()).unwrap();
    assert!(generator::server_capabilities(own, &kinds).contains("testcap"), "the server's assembly links the test capability");

    // The routed kind gate covers the test kind, and the plumbing names it
    // nowhere: the server and the engine read it from the table.
    let routed = gates::routed_kinds(&serde_json::json!({ "kinds": table }));
    let test_kind: Vec<_> = routed.into_iter().filter(|k| k.kind == "test_kind").collect();
    assert_eq!(test_kind.len(), 1);
    for file in [
        "server/logs/logs.go",
        "server/logs/read.go",
        "server/logs/receive.go",
        "server/logs/routes.go",
        "server/logs/seed.go",
        "server/kinds/kinds.go",
        "server/kindroutes/kindroutes.go",
        "server/plans/plans.go",
        "server/kinds.go",
        "server/main.go",
        "web/src/engine.ts",
        "tools/caps-run.mjs",
    ] {
        let source = std::fs::read_to_string(root.join(file)).unwrap();
        assert_eq!(gates::kind_name_violations(file, &source, &test_kind), Vec::<String>::new());
    }

    let docs = generator::read_docs(&declared());
    assert_eq!(docs.ops.iter().map(|(op, _)| op.as_str()).collect::<Vec<_>>(), ["validate", "receive"]);

    let answer = cap_abi::kernel::invoke(&[&CAP], r#"{"op":"validate"}"#);
    assert_eq!(answer, r#"{"ok":{"op":"validate"}}"#);
    let named = cap_abi::kernel::invoke(&[&CAP], r#"{"cap":"testcap","op":"receive"}"#);
    assert_eq!(named, r#"{"ok":{"op":"receive"}}"#);
}
