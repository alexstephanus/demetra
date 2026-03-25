fn main() {
    println!("cargo:warning=Compiling slint assets");
    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer)
        .with_library_paths(std::collections::HashMap::from([(
            "ui".to_string(),
            std::path::PathBuf::from("ui"),
        )]));
    slint_build::compile_with_config("ui/main.slint", config).unwrap();
    println!("cargo:warning=Finished compiling slint");
}
