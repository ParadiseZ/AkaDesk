use crate::codec::{read_envelope, write_envelope};
use interprocess::local_socket::{prelude::*, GenericFilePath, GenericNamespaced, ListenerOptions};
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub trait IpcRequestHandler: Send + Sync + 'static {
    fn initial_events(&self) -> io::Result<Vec<crate::types::IpcEnvelope>>;

    fn subscribe_events(&self) -> io::Result<Option<EventSubscription>> {
        Ok(None)
    }

    fn handle_request(
        &self,
        envelope: crate::types::IpcEnvelope,
    ) -> io::Result<crate::types::IpcEnvelope>;
}

pub struct EventSubscription {
    pub receiver: Receiver<crate::types::IpcEnvelope>,
    pub alive: Arc<AtomicBool>,
}

impl Drop for EventSubscription {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
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
        let stream = stream?;
        let handler = Arc::clone(&handler);
        thread::spawn(move || {
            if let Err(err) = handle_connection(stream, handler) {
                eprintln!("host-ipc connection error: {err}");
            }
        });
    }

    Ok(())
}

fn handle_connection(
    stream: interprocess::local_socket::Stream,
    handler: Arc<dyn IpcRequestHandler>,
) -> io::Result<()> {
    let (mut recv_half, mut send_half) = stream.split();
    for event in handler.initial_events()? {
        write_envelope(&mut send_half, &event)?;
    }

    let (outbound_tx, outbound_rx) = mpsc::channel::<crate::types::IpcEnvelope>();
    let alive = Arc::new(AtomicBool::new(true));

    let writer_alive = Arc::clone(&alive);
    let writer = thread::spawn(move || {
        while let Ok(envelope) = outbound_rx.recv() {
            if write_envelope(&mut send_half, &envelope).is_err() {
                break;
            }
        }
        writer_alive.store(false, Ordering::Relaxed);
    });

    let event_forwarder = if let Some(subscription) = handler.subscribe_events()? {
        let alive = Arc::clone(&alive);
        let outbound_tx = outbound_tx.clone();
        Some(thread::spawn(move || {
            while alive.load(Ordering::Relaxed) {
                match subscription
                    .receiver
                    .recv_timeout(Duration::from_millis(250))
                {
                    Ok(envelope) => {
                        if outbound_tx.send(envelope).is_err() {
                            break;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        }))
    } else {
        None
    };

    let result = (|| -> io::Result<()> {
        loop {
            let envelope = match read_envelope(&mut recv_half) {
                Ok(envelope) => envelope,
                Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(err) if err.kind() == io::ErrorKind::ConnectionReset => break,
                Err(err) => return Err(err),
            };

            let response = handler.handle_request(envelope)?;
            if outbound_tx.send(response).is_err() {
                break;
            }
        }
        Ok(())
    })();

    alive.store(false, Ordering::Relaxed);
    drop(outbound_tx);
    let _ = writer.join();
    if let Some(event_forwarder) = event_forwarder {
        let _ = event_forwarder.join();
    }
    result
}

fn socket_path(socket_name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{socket_name}.sock"))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::client::IpcClient;
    use crate::types::*;
    use prost::Message;
    use std::sync::Mutex;

    struct Handler {
        receiver: Mutex<Option<Receiver<IpcEnvelope>>>,
        alive: Arc<AtomicBool>,
        _sender: mpsc::Sender<IpcEnvelope>,
    }
    impl IpcRequestHandler for Handler {
        fn initial_events(&self) -> io::Result<Vec<IpcEnvelope>> {
            Ok(Vec::new())
        }
        fn subscribe_events(&self) -> io::Result<Option<EventSubscription>> {
            Ok(self
                .receiver
                .lock()
                .unwrap()
                .take()
                .map(|receiver| EventSubscription {
                    receiver,
                    alive: Arc::clone(&self.alive),
                }))
        }
        fn handle_request(&self, envelope: IpcEnvelope) -> io::Result<IpcEnvelope> {
            Ok(IpcEnvelope {
                request_id: envelope.request_id,
                kind: IpcMessageKind::Response as i32,
                message_type: RUNTIME_STATUS_RESPONSE.into(),
                payload: RuntimeStatusResponse {
                    status: None,
                    error: None,
                }
                .encode_to_vec(),
                timestamp_ms: 0,
            })
        }
    }

    #[test]
    fn closing_idle_ipc_client_cancels_subscription_and_joins_writer() {
        let name = format!(
            "akadesk-ipc-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let listener = ListenerOptions::new()
            .name(name.clone().to_ns_name::<GenericNamespaced>().unwrap())
            .create_sync()
            .unwrap();
        let (sender, receiver) = mpsc::channel();
        let alive = Arc::new(AtomicBool::new(true));
        let handler = Arc::new(Handler {
            receiver: Mutex::new(Some(receiver)),
            alive: Arc::clone(&alive),
            _sender: sender,
        });
        let (done_tx, done_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let stream = listener.accept().unwrap();
            done_tx.send(handle_connection(stream, handler)).unwrap();
        });
        let mut client = IpcClient::connect(&name).unwrap();
        client.get_runtime_status().unwrap();
        drop(client);
        assert!(done_rx
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .is_ok());
        assert!(!alive.load(Ordering::Relaxed));
        worker.join().unwrap();
    }
}
