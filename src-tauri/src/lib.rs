mod filter;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            filter::filter_photos,
            filter::write_report,
            filter::open_dir
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
