use crate::generated::v1::{DevicePresence, PlatformKind};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, Ipv4Addr, UdpSocket};

pub const AKADESK_SERVICE_TYPE: &str = "_akadesk._tcp.local.";

pub fn publish_presence(presence: &DevicePresence) -> io::Result<ServiceDaemon> {
    let daemon = ServiceDaemon::new().map_err(to_io_error)?;
    let service_name = format!(
        "{}-{}",
        presence
            .identity
            .as_ref()
            .map(|it| it.display_name.clone())
            .unwrap_or_else(|| "AkaDesk".to_string()),
        presence
            .identity
            .as_ref()
            .map(|it| it.device_id.clone())
            .unwrap_or_else(|| "device".to_string())
    );
    let host_name = format!(
        "{}.local.",
        presence
            .identity
            .as_ref()
            .map(|it| it.hostname.clone())
            .unwrap_or_else(|| "akadesk-host".to_string())
    );
    let ip = detect_local_ip().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let properties = HashMap::from([
        (
            "device_id".to_string(),
            presence
                .identity
                .as_ref()
                .map(|it| it.device_id.clone())
                .unwrap_or_default(),
        ),
        (
            "display_name".to_string(),
            presence
                .identity
                .as_ref()
                .map(|it| it.display_name.clone())
                .unwrap_or_default(),
        ),
        (
            "platform".to_string(),
            platform_label(
                presence
                    .identity
                    .as_ref()
                    .map(|it| it.platform)
                    .unwrap_or(PlatformKind::Unspecified as i32),
            ),
        ),
        (
            "hostname".to_string(),
            presence
                .identity
                .as_ref()
                .map(|it| it.hostname.clone())
                .unwrap_or_default(),
        ),
        (
            "app_version".to_string(),
            presence
                .identity
                .as_ref()
                .map(|it| it.app_version.clone())
                .unwrap_or_default(),
        ),
        (
            "control_port".to_string(),
            presence.control_port.to_string(),
        ),
        ("paired".to_string(), presence.paired.to_string()),
        ("reachable".to_string(), presence.reachable.to_string()),
        (
            "last_seen_ms".to_string(),
            presence.last_seen_ms.to_string(),
        ),
    ]);
    let service = ServiceInfo::new(
        AKADESK_SERVICE_TYPE,
        &service_name,
        &host_name,
        ip,
        presence.control_port as u16,
        properties,
    )
    .map_err(to_io_error)?;
    daemon.register(service).map_err(to_io_error)?;
    Ok(daemon)
}

fn detect_local_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip())
}

fn platform_label(platform: i32) -> String {
    if platform == PlatformKind::Windows as i32 {
        "windows".to_string()
    } else if platform == PlatformKind::Android as i32 {
        "android".to_string()
    } else if platform == PlatformKind::Linux as i32 {
        "linux".to_string()
    } else {
        "unknown".to_string()
    }
}

fn to_io_error(err: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::Other, err.to_string())
}
