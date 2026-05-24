use crate::codec::{read_envelope, write_envelope};
use interprocess::local_socket::{prelude::*, GenericFilePath, GenericNamespaced, ListenerOptions};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

pub trait IpcRequestHandler: Send + Sync + 'static {
    fn initial_events(&self) -> io::Result<Vec<crate::types::IpcEnvelope>>;

    fn handle_request(
        &self,
        envelope: crate::types::IpcEnvelope,
    ) -> io::Result<crate::types::IpcEnvelope>;
}

pub fn serve_forever(socket_name: &str, handler: Arc<dyn IpcRequestHandler>) -> io::Result<()> {
    let listener = if GenericNamespaced::is_supported() {
        ListenerOptions::new()
            .name(socket_name.to_ns_name::<GenericNamespaced>()?)
            .try_overwrite(true)
            .create_sync()?
    } else {
        let path = socket_path(socket_name);
        ListenerOptions::new()
            .name(path.to_fs_name::<GenericFilePath>()?)
            .try_overwrite(true)
            .create_sync()?
    };

    for stream in listener.incoming() {
        let mut stream = stream?;
        for event in handler.initial_events()? {
            write_envelope(&mut stream, &event)?;
        }

        loop {
            let envelope = match read_envelope(&mut stream) {
                Ok(envelope) => envelope,
                Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(err) if err.kind() == io::ErrorKind::ConnectionReset => break,
                Err(err) => return Err(err),
            };

            let response = handler.handle_request(envelope)?;
            write_envelope(&mut stream, &response)?;
        }
    }

    Ok(())
}

fn socket_path(socket_name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{socket_name}.sock"))
}
