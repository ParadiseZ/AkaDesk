#[derive(Debug, Default)]
pub struct WindowsCaptureBackendPlan {
    pub prefer_wgc: bool,
    pub allow_dxgi_fallback: bool,
}
