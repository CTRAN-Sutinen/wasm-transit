fn main() {
    let protoc = protoc_bin_vendored::protoc_bin_path().expect("vendored protoc");
    // A build script runs on one thread, so setting PROTOC here is safe.
    std::env::set_var("PROTOC", protoc);

    prost_build::compile_protos(&["proto/gtfs-realtime.proto"], &["proto/"])
        .expect("compile gtfs-realtime.proto");

    println!("cargo:rerun-if-changed=proto/gtfs-realtime.proto");
}
