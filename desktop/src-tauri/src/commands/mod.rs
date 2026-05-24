use crate::state::{
    AppState, HostSettingsInput, RemoteConnectRequest, ShellActionReceipt, ShellSettings,
    ShellSnapshot,
};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn bootstrap_shell(state: State<Arc<AppState>>) -> Result<ShellSnapshot, String> {
    state.bootstrap_snapshot().map_err(|err| {
        state.log_command_error("BOOTSTRAP_SHELL_FAILED", &err);
        err
    })
}

#[tauri::command]
pub fn refresh_shell(state: State<Arc<AppState>>) -> Result<ShellSnapshot, String> {
    state.bootstrap_snapshot().map_err(|err| {
        state.log_command_error("REFRESH_SHELL_FAILED", &err);
        err
    })
}

#[tauri::command]
pub fn save_shell_settings(
    app: AppHandle,
    state: State<Arc<AppState>>,
    settings: ShellSettings,
) -> Result<ShellSnapshot, String> {
    snapshot_command(&app, state, "SAVE_SHELL_SETTINGS_FAILED", move |app_state| {
        app_state.save_shell_settings(settings)
    })
}

#[tauri::command]
pub fn update_host_settings(
    app: AppHandle,
    state: State<Arc<AppState>>,
    settings: HostSettingsInput,
) -> Result<ShellSnapshot, String> {
    snapshot_command(&app, state, "UPDATE_HOST_SETTINGS_FAILED", move |app_state| {
        app_state.update_host_settings(settings)
    })
}

#[tauri::command]
pub fn refresh_temporary_verification_code(
    app: AppHandle,
    state: State<Arc<AppState>>,
) -> Result<ShellSnapshot, String> {
    snapshot_command(
        &app,
        state,
        "REFRESH_TEMPORARY_VERIFICATION_CODE_FAILED",
        |app_state| app_state.refresh_temporary_verification_code(),
    )
}

#[tauri::command]
pub fn queue_remote_connect(
    state: State<Arc<AppState>>,
    request: RemoteConnectRequest,
) -> Result<ShellActionReceipt, String> {
    state.queue_remote_connect(request).map_err(|err| {
        state.log_command_error("QUEUE_REMOTE_CONNECT_FAILED", &err);
        err
    })
}

#[tauri::command]
pub fn preview_incoming_request(
    app: AppHandle,
    state: State<Arc<AppState>>,
) -> Result<ShellSnapshot, String> {
    snapshot_command(&app, state, "PREVIEW_INCOMING_REQUEST_FAILED", |app_state| {
        app_state.preview_incoming_request()
    })
}

#[tauri::command]
pub fn respond_incoming_request(
    app: AppHandle,
    state: State<Arc<AppState>>,
    allow: bool,
) -> Result<ShellSnapshot, String> {
    snapshot_command(&app, state, "RESPOND_INCOMING_REQUEST_FAILED", move |app_state| {
        app_state.respond_incoming_request(allow)
    })
}

#[tauri::command]
pub fn disconnect_session(
    app: AppHandle,
    state: State<Arc<AppState>>,
) -> Result<ShellSnapshot, String> {
    snapshot_command(&app, state, "DISCONNECT_SESSION_FAILED", |app_state| {
        app_state.disconnect_session()
    })
}

fn emit_snapshot(app: &AppHandle, snapshot: &ShellSnapshot) {
    let _ = app.emit("shell://state", snapshot);
}

fn snapshot_command<F>(
    app: &AppHandle,
    state: State<Arc<AppState>>,
    error_code: &str,
    action: F,
) -> Result<ShellSnapshot, String>
where
    F: FnOnce(&AppState) -> Result<ShellSnapshot, String>,
{
    match action(state.inner().as_ref()) {
        Ok(snapshot) => {
            emit_snapshot(app, &snapshot);
            Ok(snapshot)
        }
        Err(err) => {
            state.log_command_error(error_code, &err);
            Err(err)
        }
    }
}
