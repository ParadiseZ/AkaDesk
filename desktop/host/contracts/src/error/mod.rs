use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostErrorCode {
    InvalidRequest,
    Unsupported,
    Unavailable,
    Timeout,
    Conflict,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryClass {
    Never,
    SafeImmediate,
    RetryAfterBackoff,
}

#[derive(Debug, Clone)]
pub struct HostError {
    pub code: HostErrorCode,
    pub retry: RetryClass,
    pub message: String,
}

impl HostError {
    pub fn new(code: HostErrorCode, retry: RetryClass, message: impl Into<String>) -> Self {
        Self {
            code,
            retry,
            message: message.into(),
        }
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for HostError {}

pub type HostResult<T> = Result<T, HostError>;
