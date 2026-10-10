fn main() {
    let proto_dir = "../../../proto";
    println!("cargo:rerun-if-changed={proto_dir}");
    prost_build::Config::new()
        .compile_protos(&[format!("{proto_dir}/ipc.proto")], &[proto_dir])
        .expect("failed to compile IPC protocol");
}
