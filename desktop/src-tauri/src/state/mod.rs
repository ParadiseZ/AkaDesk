#[derive(Debug, Default)]
pub struct AppHostState {
    pub label: &'static str,
}

impl AppHostState {
    pub fn ready() -> Self {
        Self { label: "READY_FOR_M2" }
    }
}
