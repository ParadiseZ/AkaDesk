#[derive(Debug, Default)]
pub struct PlatformPorts {
    pub has_capture: bool,
    pub has_audio: bool,
    pub has_encode: bool,
    pub has_display: bool,
    pub has_input: bool,
}
