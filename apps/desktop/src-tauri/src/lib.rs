use eutheto_types::FoundationStatus;

#[tauri::command]
fn app_get_foundation_status() -> FoundationStatus {
    eutheto_core::foundation_status()
}

/// Runs the Tauri desktop application.
///
/// # Errors
///
/// Returns the error from [`tauri::Builder::run`] if Tauri cannot start or run
/// the application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![app_get_foundation_status])
        .run(tauri::generate_context!())
}

#[cfg(test)]
mod tests {
    use super::app_get_foundation_status;

    #[test]
    fn command_delegates_to_authoritative_core_status() {
        assert_eq!(
            app_get_foundation_status(),
            eutheto_core::foundation_status()
        );
    }
}
