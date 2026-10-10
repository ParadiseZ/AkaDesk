use rand::distributions::{Alphanumeric, DistString};
use rand::Rng;
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
            let mut identity: Self = serde_json::from_str(&text)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
            let numeric_id = numeric_device_id(&identity.device_id);
            if identity.device_id != numeric_id {
                identity.device_id = numeric_id;
                write_json(&path, &identity)?;
            }
            return Ok(identity);
        }

        let hostname = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "AkaDesk-Host".to_string());
        let identity = Self {
            device_id: rand::thread_rng()
                .gen_range(100_000_000u64..1_000_000_000)
                .to_string(),
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
        let file: PairingStoreFile = if path.exists() {
            let text = fs::read_to_string(&path)?;
            serde_json::from_str(&text)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?
        } else {
            PairingStoreFile::default()
        };
        let mut migrated = PairingStoreFile::default();
        let mut changed = false;
        for (id, mut record) in file.records {
            let numeric_id = numeric_device_id(&id);
            changed |= numeric_id != id;
            record.controller_device_id = numeric_id.clone();
            if migrated.records.insert(numeric_id, record).is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "device ID migration collision",
                ));
            }
        }
        if changed {
            write_json(&path, &migrated)?;
        }
        Ok(Self {
            path,
            file: migrated,
        })
    }

    pub fn upsert_pairing(
        &mut self,
        controller_device_id: &str,
        controller_display_name: &str,
        pair_token: &str,
    ) -> io::Result<()> {
        let now = now_ms();
        let mut file = self.file.clone();
        file.records.insert(
            controller_device_id.to_string(),
            PairingRecord {
                controller_device_id: controller_device_id.to_string(),
                controller_display_name: controller_display_name.to_string(),
                pair_token_hash: hash_token(pair_token),
                created_at_ms: now,
                last_used_at_ms: now,
            },
        );
        write_json(&self.path, &file)?;
        self.file = file;
        Ok(())
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

    pub fn count(&self) -> usize {
        self.file.records.len()
    }

    pub fn controller_display_name(&self, device_id: &str) -> Option<String> {
        self.file
            .records
            .get(device_id)
            .map(|record| record.controller_display_name.clone())
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
    pub expires_at_ms: i64,
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
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    loop {
        let code: String = (0..8)
            .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
            .collect();
        if is_mixed_verification_code(&code) {
            return code;
        }
    }
}

pub fn is_mixed_verification_code(code: &str) -> bool {
    (6..=8).contains(&code.len())
        && code
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && code.bytes().any(|c| c.is_ascii_uppercase())
        && code.bytes().any(|c| c.is_ascii_digit())
}

/// Shared with Android migration: SHA-256, first 8 bytes big endian, 9 decimal digits.
pub fn numeric_device_id(old_id: &str) -> String {
    if old_id.len() == 9 && old_id.bytes().all(|c| c.is_ascii_digit()) {
        return old_id.to_owned();
    }
    let digest = Sha256::digest(old_id.as_bytes());
    let value = u64::from_be_bytes(digest[..8].try_into().expect("SHA-256 prefix"));
    (100_000_000 + value % 900_000_000).to_string()
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
    host_ipc::storage::atomic_write(path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_identity_migration_matches_android_and_preserves_pairing() {
        assert_eq!(numeric_device_id("fqeUEkVCScqa53y7Ytv47t88"), "453414118");
        assert_eq!(
            numeric_device_id("android-12345678-1234-1234-1234-123456789012"),
            "916537989"
        );
        assert_eq!(numeric_device_id("012345678"), "012345678");
        let dir = std::env::temp_dir().join(format!("akadesk-id-migration-{}", random_string(16)));
        fs::create_dir_all(&dir).unwrap();
        let identity = HostIdentity {
            device_id: "fqeUEkVCScqa53y7Ytv47t88".into(),
            display_name: "original name".into(),
            hostname: "pc".into(),
            app_version: "test".into(),
            platform: "windows".into(),
        };
        write_json(&dir.join("host_identity.json"), &identity).unwrap();
        let migrated = HostIdentity::load_or_create(&dir).unwrap();
        assert_eq!(migrated.device_id, "453414118");
        assert_eq!(migrated.display_name, "original name");
        assert_eq!(
            HostIdentity::load_or_create(&dir).unwrap().device_id,
            migrated.device_id
        );
        let mut pairings = PairingStore::load(&dir).unwrap();
        pairings
            .upsert_pairing(
                "android-12345678-1234-1234-1234-123456789012",
                "phone",
                "secret-token",
            )
            .unwrap();
        let mut migrated = PairingStore::load(&dir).unwrap();
        assert_eq!(migrated.count(), 1);
        assert!(migrated
            .verify_pair_token("916537989", "secret-token")
            .unwrap());
        fs::remove_file(dir.join("host_identity.json")).unwrap();
        fs::remove_file(dir.join("pairings.json")).unwrap();
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn generated_codes_always_mix_letters_and_digits() {
        for _ in 0..100 {
            let code = generate_verification_code();
            assert_eq!(code.len(), 8);
            assert!(is_mixed_verification_code(&code));
        }
        assert!(!is_mixed_verification_code("12345678"));
        assert!(!is_mixed_verification_code("ABCDEFGH"));
        assert!(!is_mixed_verification_code("ABC12!"));
    }
}
