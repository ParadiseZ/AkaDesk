use crate::types::IpcEnvelope;

#[derive(Debug, Default)]
pub struct IpcServer {
    listening: bool,
}

impl IpcServer {
    pub fn start(&mut self) {
        self.listening = true;
    }

    pub fn is_listening(&self) -> bool {
        self.listening
    }

    pub fn handle(&self, envelope: IpcEnvelope) -> IpcEnvelope {
        envelope
    }
}
