use loq_core::Capabilities;

/// Return the full read-only capability manifest.
#[tauri::command]
fn get_capabilities() -> Capabilities {
    loq_core::probe()
}

/// Return the crate version so the UI can display it.
#[tauri::command]
fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_capabilities, get_version])
        .run(tauri::generate_context!())
        .expect("failed to start LOQ Power");
}
