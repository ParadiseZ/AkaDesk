use crate::events::SharedStateSignal;
use crate::pairing::{generate_verification_code, is_mixed_verification_code, now_ms};
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
            custom_verification_code: String::new(),
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
    signal: Option<SharedStateSignal>,
    last_finished_session_id: Option<String>,
}

impl HostSettingsStore {
    pub fn load(base_dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(base_dir)?;
        let path = base_dir.join("host_settings.json");
        let mut settings: HostSettings = if path.exists() {
            let text = fs::read_to_string(&path)?;
            serde_json::from_str(&text)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?
        } else {
            let settings = HostSettings::default();
            write_json(&path, &settings)?;
            settings
        };
        let temporary = normalize_temporary_code(settings.temporary_verification_code.clone());
        let custom = normalize_custom_code(settings.custom_verification_code.clone());
        let custom = if !custom.is_empty() && !is_mixed_verification_code(&custom) {
            generate_verification_code()
        } else {
            custom
        };
        if temporary != settings.temporary_verification_code
            || custom != settings.custom_verification_code
        {
            settings.temporary_verification_code = temporary;
            settings.custom_verification_code = custom;
            settings.updated_at_ms = now_ms();
            write_json(&path, &settings)?;
        }
        Ok(Self {
            path,
            settings,
            signal: None,
            last_finished_session_id: None,
        })
    }

    pub fn set_signal(&mut self, signal: SharedStateSignal) {
        self.signal = Some(signal);
    }

    pub fn snapshot(&self) -> HostSettingsSnapshot {
        self.settings.clone().into_snapshot()
    }

    pub fn current(&self) -> HostSettings {
        self.settings.clone()
    }

    pub fn update(&mut self, snapshot: HostSettingsSnapshot) -> io::Result<HostSettingsSnapshot> {
        let settings = HostSettings::from_snapshot(snapshot);
        if !settings.custom_verification_code.is_empty()
            && !is_mixed_verification_code(&settings.custom_verification_code)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "自定义验证码须为 6–8 位字母和数字组合",
            ));
        }
        if matches!(
            settings.verification_mode.as_str(),
            "CUSTOM_CODE" | "BOTH_CODES"
        ) && settings.custom_verification_code.is_empty()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "请先设置自定义验证码",
            ));
        }
        write_json(&self.path, &settings)?;
        self.settings = settings;
        if let Some(signal) = self.signal.as_ref() {
            signal.notify();
        }
        Ok(self.snapshot())
    }

    pub fn refresh_temporary_code(&mut self) -> io::Result<HostSettingsSnapshot> {
        let mut settings = self.settings.clone();
        settings.temporary_verification_code = generate_temporary_verification_code();
        settings.updated_at_ms = now_ms();
        write_json(&self.path, &settings)?;
        self.settings = settings;
        if let Some(signal) = self.signal.as_ref() {
            signal.notify();
        }
        Ok(self.snapshot())
    }

    pub fn refresh_after_session_end(&mut self, session_id: &str) -> io::Result<()> {
        if self.last_finished_session_id.as_deref() == Some(session_id) {
            return Ok(());
        }
        if self.settings.refresh_temporary_code_after_controlled_end {
            self.refresh_temporary_code()?;
        }
        self.last_finished_session_id = Some(session_id.to_owned());
        Ok(())
    }
}

fn write_json(path: &Path, value: &HostSettings) -> io::Result<()> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
    host_ipc::storage::atomic_write(path, text.as_bytes())
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
    let normalized = value.trim().to_ascii_uppercase();
    if !is_mixed_verification_code(&normalized) {
        generate_temporary_verification_code()
    } else {
        normalized
    }
}

fn normalize_custom_code(value: String) -> String {
    let normalized = value.trim().to_ascii_uppercase();
    if normalized.is_empty() {
        String::new()
    } else {
        normalized
    }
}

fn normalize_submitted_code(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}

fn generate_temporary_verification_code() -> String {
    generate_verification_code()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_codes_migrate_once_and_invalid_custom_updates_do_not_commit() {
        let dir = std::env::temp_dir().join(format!(
            "akadesk-code-migration-{}",
            crate::pairing::random_string(16)
        ));
        fs::create_dir_all(&dir).unwrap();
        let mut original = HostSettings::default();
        original.temporary_verification_code = "123456".into();
        original.custom_verification_code = "AKADESK".into();
        original.verification_mode = "BOTH_CODES".into();
        write_json(&dir.join("host_settings.json"), &original).unwrap();
        let mut store = HostSettingsStore::load(&dir).unwrap();
        let settings = store.current();
        assert!(is_mixed_verification_code(
            &settings.temporary_verification_code
        ));
        assert!(is_mixed_verification_code(
            &settings.custom_verification_code
        ));
        assert_eq!(
            HostSettingsStore::load(&dir)
                .unwrap()
                .current()
                .temporary_verification_code,
            settings.temporary_verification_code
        );
        assert!(settings
            .accepts_access_code(&settings.temporary_verification_code.to_ascii_lowercase()));
        let mut input = store.snapshot();
        input.custom_verification_code = "12345678".into();
        assert!(store.update(input).is_err());
        assert_eq!(
            store.current().custom_verification_code,
            settings.custom_verification_code
        );
        assert_eq!(
            HostSettingsStore::load(&dir)
                .unwrap()
                .current()
                .custom_verification_code,
            settings.custom_verification_code
        );
        fs::remove_file(dir.join("host_settings.json")).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
