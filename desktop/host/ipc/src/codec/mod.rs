use crate::types::IpcEnvelope;
use prost::Message;
use std::io::{self, Read, Write};

fn prost_err(err: prost::DecodeError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, err.to_string())
}

pub fn write_envelope<W: Write>(writer: &mut W, envelope: &IpcEnvelope) -> io::Result<()> {
    let bytes = envelope.encode_to_vec();
    let len = u32::try_from(bytes.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "IPC message too large"))?;
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()?;
    Ok(())
}

pub fn read_envelope<R: Read>(reader: &mut R) -> io::Result<IpcEnvelope> {
    let mut len_bytes = [0u8; 4];
    reader.read_exact(&mut len_bytes)?;
    let len = u32::from_le_bytes(len_bytes) as usize;
    if len > 4 * 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "IPC message exceeds the size limit",
        ));
    }
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload)?;
    IpcEnvelope::decode(&payload[..]).map_err(prost_err)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oversized_length_is_rejected_before_reading_or_allocating_payload() {
        let mut input = &u32::MAX.to_le_bytes()[..];
        assert_eq!(
            read_envelope(&mut input).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}
