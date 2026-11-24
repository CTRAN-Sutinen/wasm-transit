//! Generate Rust types from the official GTFS-realtime spec.
//!
//! protoc is vendored through `protoc-bin-vendored`. Building this crate
//! needs no system-wide protobuf installation.

fn main() {
    let protoc = protoc_bin_vendored::protoc_bin_path().expect("vendored protoc");
    // Build scripts run on one thread. Setting an env var here is fine for
    // prost-build's protoc lookup.
    std::env::set_var("PROTOC", protoc);

    prost_build::compile_protos(&["proto/gtfs-realtime.proto"], &["proto/"])
        .expect("compile gtfs-realtime.proto");

    println!("cargo:rerun-if-changed=proto/gtfs-realtime.proto");
}
