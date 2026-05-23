use crate::types::IpcEnvelope;

#[derive(Debug, Default)]
pub struct IpcClient {
    connected: bool,
}

impl IpcClient {
    pub fn connect(&mut self) {
        self.connected = true;
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn send(&self, _envelope: IpcEnvelope) {
        // M1 only establishes the client-side shape.
    }
}
