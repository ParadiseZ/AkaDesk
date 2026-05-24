pub mod commands;
pub mod diagnostics;
pub mod host;
pub mod state;

use crate::state::AppState;
use std::sync::Arc;
use std::time::Duration;
use tauri::{Emitter, Manager};

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|err| -> Box<dyn std::error::Error> { Box::new(err) })?;
            let state = Arc::new(AppState::load(app_data_dir)?);
            state.ensure_host_started().map_err(into_boxed_error)?;
            app.manage(Arc::clone(&state));
            start_sync_loop(app.handle().clone(), state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap_shell,
            commands::refresh_shell,
            commands::save_shell_settings,
            commands::update_host_settings,
            commands::refresh_temporary_verification_code,
            commands::queue_remote_connect,
            commands::preview_incoming_request,
            commands::respond_incoming_request,
            commands::disconnect_session
        ])
        .run(tauri::generate_context!())
        .expect("failed to run AkaDesk desktop shell");
}

fn start_sync_loop(app: tauri::AppHandle, state: Arc<AppState>) {
    std::thread::spawn(move || loop {
        if let Ok(snapshot) = state.bootstrap_snapshot() {
            let _ = app.emit("shell://state", snapshot);
        }
        std::thread::sleep(Duration::from_secs(2));
    });
}

fn into_boxed_error(message: String) -> Box<dyn std::error::Error> {
    Box::new(std::io::Error::new(std::io::ErrorKind::Other, message))
}
