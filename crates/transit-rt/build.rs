//! Generate Rust types from the official GTFS-realtime spec.
//!
//! protoc itself is vendored via `protoc-bin-vendored`, so building this
//! crate needs no system-wide protobuf installation.

fn main() {
    let protoc = protoc_bin_vendored::protoc_bin_path().expect("vendored protoc");
    // SAFETY-ish: build scripts are single-threaded; setting env vars in
    // build scripts is fine for prost-build's protoc lookup.
    std::env::set_var("PROTOC", protoc);

    prost_build::compile_protos(&["proto/gtfs-realtime.proto"], &["proto/"])
        .expect("compile gtfs-realtime.proto");

    println!("cargo:rerun-if-changed=proto/gtfs-realtime.proto");
}
