include!("src/generated_command_catalog.rs");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(REGISTERED_COMMANDS)),
    )?;
    Ok(())
}
