use crate::pairing::now_ms;
use host_ipc::types::HostSettingsSnapshot;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConnectionMethod {
    AccessCode,
    TrustOnly,
    AccessCodeAndConsent,
}

impl Default for ConnectionMethod {
    fn default() -> Self {
        Self::AccessCode
    }
}

impl ConnectionMethod {
    pub fn as_wire_value(self) -> &'static str {
        match self {
            Self::AccessCode => "ACCESS_CODE",
            Self::TrustOnly => "TRUST_ONLY",
            Self::AccessCodeAndConsent => "ACCESS_CODE_AND_CONSENT",
        }
    }

    pub fn from_wire_value(value: impl AsRef<str>) -> Self {
        match value.as_ref() {
            "TRUST_ONLY" => Self::TrustOnly,
            "ACCESS_CODE_AND_CONSENT" => Self::AccessCodeAndConsent,
            _ => Self::AccessCode,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HostSettings {
    pub allow_remote_assistance: bool,
    pub verification_mode: String,
    pub connection_method: ConnectionMethod,
    pub notify_on_incoming_request: bool,
    pub auto_accept_trusted_controller: bool,
    pub temporary_verification_code: String,
    pub custom_verification_code: String,
    pub refresh_temporary_code_after_controlled_end: bool,
    pub updated_at_ms: i64,
}

impl Default for HostSettings {
    fn default() -> Self {
        Self {
            allow_remote_assistance: true,
            verification_mode: default_verification_mode(),
            connection_method: ConnectionMethod::default(),
            notify_on_incoming_request: true,
            auto_accept_trusted_controller: false,
            temporary_verification_code: generate_temporary_verification_code(),
            custom_verification_code: "AKADESK".to_string(),
            refresh_temporary_code_after_controlled_end: true,
            updated_at_ms: now_ms(),
        }
    }
}

impl HostSettings {
    pub fn connection_method(&self) -> ConnectionMethod {
        self.connection_method
    }

    pub fn accepts_access_code(&self, submitted_code: &str) -> bool {
        let normalized = normalize_submitted_code(submitted_code);
        if normalized.is_empty() {
            return false;
        }
        match self.verification_mode.as_str() {
            "CUSTOM_CODE" => normalized == self.custom_verification_code,
            "BOTH_CODES" => {
                normalized == self.temporary_verification_code
                    || normalized == self.custom_verification_code
            }
            _ => normalized == self.temporary_verification_code,
        }
    }

    pub fn into_snapshot(self) -> HostSettingsSnapshot {
        HostSettingsSnapshot {
            allow_remote_assistance: self.allow_remote_assistance,
            verification_mode: self.verification_mode,
            connection_method: self.connection_method.as_wire_value().to_string(),
            notify_on_incoming_request: self.notify_on_incoming_request,
            auto_accept_trusted_controller: self.auto_accept_trusted_controller,
            temporary_verification_code: self.temporary_verification_code,
            custom_verification_code: self.custom_verification_code,
            refresh_temporary_code_after_controlled_end: self
                .refresh_temporary_code_after_controlled_end,
            updated_at_ms: self.updated_at_ms,
        }
    }

    pub fn from_snapshot(snapshot: HostSettingsSnapshot) -> Self {
        Self {
            allow_remote_assistance: snapshot.allow_remote_assistance,
            verification_mode: normalize_verification_mode(snapshot.verification_mode),
            connection_method: ConnectionMethod::from_wire_value(snapshot.connection_method),
            notify_on_incoming_request: snapshot.notify_on_incoming_request,
            auto_accept_trusted_controller: snapshot.auto_accept_trusted_controller,
            temporary_verification_code: normalize_temporary_code(
                snapshot.temporary_verification_code,
            ),
            custom_verification_code: normalize_custom_code(snapshot.custom_verification_code),
            refresh_temporary_code_after_controlled_end: snapshot
                .refresh_temporary_code_after_controlled_end,
            updated_at_ms: now_ms(),
        }
    }
}

#[derive(Debug)]
pub struct HostSettingsStore {
    path: PathBuf,
    settings: HostSettings,
}

impl HostSettingsStore {
    pub fn load(base_dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(base_dir)?;
        let path = base_dir.join("host_settings.json");
        let settings = if path.exists() {
            let text = fs::read_to_string(&path)?;
            serde_json::from_str(&text)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?
        } else {
            let settings = HostSettings::default();
            write_json(&path, &settings)?;
            settings
        };
        Ok(Self { path, settings })
    }

    pub fn snapshot(&self) -> HostSettingsSnapshot {
        self.settings.clone().into_snapshot()
    }

    pub fn current(&self) -> HostSettings {
        self.settings.clone()
    }

    pub fn update(&mut self, snapshot: HostSettingsSnapshot) -> io::Result<HostSettingsSnapshot> {
        self.settings = HostSettings::from_snapshot(snapshot);
        write_json(&self.path, &self.settings)?;
        Ok(self.snapshot())
    }

    pub fn refresh_temporary_code(&mut self) -> io::Result<HostSettingsSnapshot> {
        self.settings.temporary_verification_code = generate_temporary_verification_code();
        self.settings.updated_at_ms = now_ms();
        write_json(&self.path, &self.settings)?;
        Ok(self.snapshot())
    }
}

fn write_json(path: &Path, value: &HostSettings) -> io::Result<()> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
    fs::write(path, text)
}

fn default_verification_mode() -> String {
    "TEMPORARY_CODE".to_string()
}

fn normalize_verification_mode(value: String) -> String {
    match value.as_str() {
        "TEMP_CODE_ONLY" => "TEMPORARY_CODE".to_string(),
        "TRUSTED_OR_TEMP_CODE" => "BOTH_CODES".to_string(),
        "TRUSTED_ONLY" => "CUSTOM_CODE".to_string(),
        "TEMPORARY_CODE" | "CUSTOM_CODE" | "BOTH_CODES" => value,
        _ => default_verification_mode(),
    }
}

fn normalize_temporary_code(value: String) -> String {
    let normalized = value.trim().to_string();
    if normalized.is_empty() {
        generate_temporary_verification_code()
    } else {
        normalized
    }
}

fn normalize_custom_code(value: String) -> String {
    let normalized = value.trim().to_ascii_uppercase();
    if normalized.is_empty() {
        "AKADESK".to_string()
    } else {
        normalized
    }
}

fn normalize_submitted_code(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}

fn generate_temporary_verification_code() -> String {
    format!("{:06}", now_ms().unsigned_abs() % 1_000_000)
}
