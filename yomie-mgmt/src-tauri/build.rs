fn main() {
    // Compile protobuf definitions (RustDesk hbb.* protocol)
    let out_dir = std::path::PathBuf::from("src/proto");
    std::fs::create_dir_all(&out_dir).expect("Failed to create proto output directory");

    prost_build::Config::new()
        .out_dir(&out_dir)
        .compile_protos(
            &["protos/rendezvous.proto", "protos/message.proto"],
            &["protos/"],
        )
        .expect("Failed to compile protobuf definitions");

    // Rename output to hbb.rs for cleaner imports
    let generated = out_dir.join("hbb.rs");
    if !generated.exists() {
        // prost may output as _.rs when package is empty or hbb.rs
        let alt = out_dir.join("_.rs");
        if alt.exists() {
            std::fs::rename(&alt, &generated).expect("Failed to rename proto output");
        }
    }

    tauri_build::build()
}
