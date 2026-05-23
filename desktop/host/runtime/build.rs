fn main() {
    let proto_dir = "../../../proto";
    let proto_files = [
        format!("{proto_dir}/common.proto"),
        format!("{proto_dir}/device.proto"),
        format!("{proto_dir}/media.proto"),
        format!("{proto_dir}/session.proto"),
        format!("{proto_dir}/pairing.proto"),
    ];

    prost_build::Config::new()
        .compile_protos(&proto_files, &[proto_dir])
        .expect("failed to compile control-plane proto files");
}
