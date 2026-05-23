#[derive(Debug, Default)]
pub struct AppHostState {
    pub label: String,
    pub runtime_status_label: String,
    pub supports_input: bool,
}

impl AppHostState {
    pub fn ready(runtime_status_label: impl Into<String>, supports_input: bool) -> Self {
        Self {
            label: "READY_FOR_M2".to_string(),
            runtime_status_label: runtime_status_label.into(),
            supports_input,
        }
    }
}
