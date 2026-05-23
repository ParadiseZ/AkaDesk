use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostIdentity {
    pub device_id: String,
    pub display_name: String,
    pub hostname: String,
    pub app_version: String,
    pub platform: String,
}

impl HostIdentity {
    pub fn load_or_create(base_dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(base_dir)?;
        let path = base_dir.join("host_identity.json");
        if path.exists() {
            let text = fs::read_to_string(&path)?;
            return serde_json::from_str(&text)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()));
        }

        let hostname = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "AkaDesk-Host".to_string());
        let identity = Self {
            device_id: random_string(24),
            display_name: hostname.clone(),
            hostname,
            app_version: "0.1.0-m3".to_string(),
            platform: "windows".to_string(),
        };
        write_json(&path, &identity)?;
        Ok(identity)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingRecord {
    pub controller_device_id: String,
    pub controller_display_name: String,
    pub pair_token_hash: String,
    pub created_at_ms: i64,
    pub last_used_at_ms: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PairingStoreFile {
    records: BTreeMap<String, PairingRecord>,
}

#[derive(Debug)]
pub struct PairingStore {
    path: PathBuf,
    file: PairingStoreFile,
}

impl PairingStore {
    pub fn load(base_dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(base_dir)?;
        let path = base_dir.join("pairings.json");
        let file = if path.exists() {
            let text = fs::read_to_string(&path)?;
            serde_json::from_str(&text)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?
        } else {
            PairingStoreFile::default()
        };
        Ok(Self { path, file })
    }

    pub fn upsert_pairing(
        &mut self,
        controller_device_id: &str,
        controller_display_name: &str,
        pair_token: &str,
    ) -> io::Result<()> {
        let now = now_ms();
        self.file.records.insert(
            controller_device_id.to_string(),
            PairingRecord {
                controller_device_id: controller_device_id.to_string(),
                controller_display_name: controller_display_name.to_string(),
                pair_token_hash: hash_token(pair_token),
                created_at_ms: now,
                last_used_at_ms: now,
            },
        );
        self.persist()
    }

    pub fn verify_pair_token(
        &mut self,
        controller_device_id: &str,
        pair_token: &str,
    ) -> io::Result<bool> {
        let expected_hash = hash_token(pair_token);
        if let Some(record) = self.file.records.get_mut(controller_device_id) {
            if record.pair_token_hash == expected_hash {
                record.last_used_at_ms = now_ms();
                self.persist()?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn is_paired(&self, controller_device_id: &str) -> bool {
        self.file.records.contains_key(controller_device_id)
    }

    fn persist(&self) -> io::Result<()> {
        write_json(&self.path, &self.file)
    }
}

#[derive(Debug, Clone)]
pub struct PendingChallenge {
    pub request_id: String,
    pub controller_device_id: String,
    pub controller_display_name: String,
    pub challenge_id: String,
    pub verification_code: String,
}

pub type SharedPairingStore = Arc<Mutex<PairingStore>>;

pub fn load_runtime_base_dir() -> PathBuf {
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local_app_data)
            .join("AkaDesk")
            .join("host-runtime");
    }
    std::env::temp_dir().join("AkaDesk").join("host-runtime")
}

pub fn random_string(len: usize) -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), len)
}

pub fn generate_pair_token() -> String {
    random_string(32)
}

pub fn generate_verification_code() -> String {
    random_string(8).to_uppercase()
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
    fs::write(path, text)
}
