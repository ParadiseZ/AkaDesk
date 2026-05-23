use crate::types::IpcEnvelope;

#[derive(Debug, Default, Clone, Copy)]
pub struct ProtoCodec;

impl ProtoCodec {
    pub fn encode(&self, envelope: &IpcEnvelope) -> Vec<u8> {
        envelope.payload.clone()
    }

    pub fn decode(&self, payload: Vec<u8>) -> IpcEnvelope {
        IpcEnvelope {
            request_id: String::new(),
            kind: crate::types::IpcMessageKind::Event,
            message_type: String::new(),
            payload,
            timestamp_ms: 0,
        }
    }
}
