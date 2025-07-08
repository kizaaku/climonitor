use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_file = "../proto/monitor.proto";
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // OUT_DIRに出力して、include_proto!で読み込む
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .file_descriptor_set_path(out_dir.join("monitor_descriptor.bin"))
        .compile(&[proto_file], &["../proto"])?;

    println!("cargo:rerun-if-changed={proto_file}");
    Ok(())
}
