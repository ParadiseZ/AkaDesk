#[derive(Debug, Default)]
pub struct WindowsDisplayBackendPlan {
    pub supports_restore: bool,
}

#[cfg(windows)]
mod native;
#[cfg(windows)]
pub use native::*;
