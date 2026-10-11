pub mod commands;
pub mod diagnostics;
pub mod host;
pub mod state;

use crate::state::AppState;
use std::sync::Arc;
use std::time::Duration;
use tauri::{Emitter, Manager};

pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|err| -> Box<dyn std::error::Error> { Box::new(err) })?;
            let app_data_dir = std::env::var_os("AKADESK_SHELL_DATA_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or(app_data_dir);
            let state = Arc::new(AppState::load(app_data_dir, app.path().resource_dir()?)?);
            if let Err(err) = state.ensure_host_started() {
                state.log_command_error("HOST_START_FAILED", &err);
            }
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
            commands::disconnect_session,
            commands::export_diagnostics
        ])
        .build(tauri::generate_context!())
        .expect("failed to build AkaDesk desktop shell");
    app.run(|handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            handle.state::<Arc<AppState>>().shutdown_host();
        }
    });
}

fn start_sync_loop(app: tauri::AppHandle, state: Arc<AppState>) {
    std::thread::spawn(move || {
        let mut retry_delay = 2u64;
        loop {
            if state.is_shutting_down() {
                break;
            }
            match state.bootstrap_snapshot() {
                Ok(snapshot) => {
                    state.record_connection_state(true);
                    retry_delay = 2;
                    let _ = app.emit("shell://state", snapshot);
                }
                Err(_error) => {
                    state.record_connection_state(false);
                    let _ = app.emit("shell://error", "电脑服务连接中断，正在重新连接");
                    retry_delay = (retry_delay * 2).min(8);
                }
            }
            std::thread::sleep(Duration::from_secs(retry_delay));
        }
    });
}
