pub mod audio;
pub mod cap;
pub mod display;
pub mod encode;
pub mod ffmpeg;
pub mod input;
#[cfg(windows)]
pub mod tools;

#[derive(Debug, Default)]
pub struct WindowsPlatformPorts;
