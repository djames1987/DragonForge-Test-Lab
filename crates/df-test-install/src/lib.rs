use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;

pub const INSTALL_MANIFEST_VERSION: u16 = 1;
pub const INSTALL_STATE_VERSION: u16 = 1;
pub const INSTALL_CONFIG_VERSION: u16 = 1;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_BINARY_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallPlatform {
    Linux,
    Windows,
}

impl InstallPlatform {
    pub fn current() -> Result<Self, InstallError> {
        match std::env::consts::OS {
            "linux" => Ok(Self::Linux),
            "windows" => Ok(Self::Windows),
            other => Err(InstallError::UnsupportedPlatform(other.to_owned())),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Windows => "windows",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallLayout {
    pub platform: InstallPlatform,
    pub binary_path: PathBuf,
    pub config_root: PathBuf,
    pub state_root: PathBuf,
    pub log_root: PathBuf,
    pub backup_root: PathBuf,
    pub install_state_path: PathBuf,
    pub service_name: String,
}

impl InstallLayout {
    pub fn production(platform: InstallPlatform) -> Self {
        match platform {
            InstallPlatform::Linux => {
                let state_root = PathBuf::from("/var/lib/dragonforge/test-lab");
                Self {
                    platform,
                    binary_path: PathBuf::from(
                        "/opt/dragonforge/test-lab/bin/dragonforge-test-lab",
                    ),
                    config_root: PathBuf::from("/etc/dragonforge/test-lab"),
                    log_root: PathBuf::from("/var/log/dragonforge/test-lab"),
                    backup_root: state_root.join("backups"),
                    install_state_path: state_root.join("install-state.json"),
                    state_root,
                    service_name: "dragonforge-test-worker.service".into(),
                }
            }
            InstallPlatform::Windows => {
                let program_data =
                    PathBuf::from(r"C:\ProgramData\DragonForge\Test Lab");
                let state_root = program_data.join("state");
                Self {
                    platform,
                    binary_path: PathBuf::from(
                        r"C:\Program Files\DragonForge\Test Lab\dragonforge-test-lab.exe",
                    ),
                    config_root: program_data.join("config"),
                    log_root: program_data.join("logs"),
                    backup_root: program_data.join("backups"),
                    install_state_path: state_root.join("install-state.json"),
                    state_root,
                    service_name: "DragonForgeTestWorker".into(),
                }
            }
        }
    }

    pub fn fixture(root: impl Into<PathBuf>, platform: InstallPlatform) -> Self {
        let root = root.into();
        let state_root = root.join("state");
        Self {
            platform,
            binary_path: root.join(if platform == InstallPlatform::Windows {
                "dragonforge-test-lab.exe"
            } else {
                "dragonforge-test-lab"
            }),
            config_root: root.join("config"),
            log_root: root.join("logs"),
            backup_root: root.join("backups"),
            install_state_path: state_root.join("install-state.json"),
            state_root,
            service_name: if platform == InstallPlatform::Windows {
                "DragonForgeTestWorker".into()
            } else {
                "dragonforge-test-worker.service".into()
            },
        }
    }

    pub fn create_data_directories(&self) -> Result<(), InstallError> {
        for path in [
            &self.config_root,
            &self.state_root,
            &self.log_root,
            &self.backup_root,
        ] {
            fs::create_dir_all(path)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseManifest {
    pub schema_version: u16,
    pub version: String,
    pub target_os: String,
    pub target_arch: String,
    pub binary_file: String,
    pub binary_sha256: String,
}

impl ReleaseManifest {
    pub fn validate(&self) -> Result<(), InstallError> {
        if self.schema_version != INSTALL_MANIFEST_VERSION {
            return Err(InstallError::UnsupportedManifestVersion(
                self.schema_version,
            ));
        }
        Version::parse(&self.version)?;
        validate_token(&self.target_os, 32)?;
        validate_token(&self.target_arch, 32)?;
        validate_leaf_name(&self.binary_file)?;
        validate_sha256(&self.binary_sha256)?;
        Ok(())
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, InstallError> {
        let bytes = read_bounded(path.as_ref(), MAX_MANIFEST_BYTES)?;
        let manifest: Self = serde_json::from_slice(&bytes)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn verify_package(
        &self,
        package_root: impl AsRef<Path>,
        expected_os: &str,
        expected_arch: &str,
    ) -> Result<PathBuf, InstallError> {
        self.validate()?;
        if self.target_os != expected_os || self.target_arch != expected_arch {
            return Err(InstallError::TargetMismatch {
                manifest_os: self.target_os.clone(),
                manifest_arch: self.target_arch.clone(),
                expected_os: expected_os.to_owned(),
                expected_arch: expected_arch.to_owned(),
            });
        }
        let binary = package_root.as_ref().join(&self.binary_file);
        if !binary.is_file() {
            return Err(InstallError::BinaryMissing(binary));
        }
        if fs::symlink_metadata(&binary)?.file_type().is_symlink() {
            return Err(InstallError::SymlinkedBinaryForbidden);
        }
        let actual = sha256_file(&binary, MAX_BINARY_BYTES)?;
        if actual != self.binary_sha256 {
            return Err(InstallError::ChecksumMismatch);
        }
        Ok(binary)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedInstallConfig {
    pub schema_version: u16,
    pub preserve_state_on_uninstall: bool,
    pub config_root: PathBuf,
    pub state_root: PathBuf,
    pub log_root: PathBuf,
}

impl ManagedInstallConfig {
    pub fn for_layout(layout: &InstallLayout) -> Self {
        Self {
            schema_version: INSTALL_CONFIG_VERSION,
            preserve_state_on_uninstall: true,
            config_root: layout.config_root.clone(),
            state_root: layout.state_root.clone(),
            log_root: layout.log_root.clone(),
        }
    }

    pub fn validate(&self) -> Result<(), InstallError> {
        if self.schema_version != INSTALL_CONFIG_VERSION {
            return Err(InstallError::UnsupportedConfigVersion(
                self.schema_version,
            ));
        }
        for path in [&self.config_root, &self.state_root, &self.log_root] {
            validate_managed_path(path)?;
        }
        Ok(())
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), InstallError> {
        self.validate()?;
        atomic_write_json(path.as_ref(), self)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, InstallError> {
        let bytes = read_bounded(path.as_ref(), MAX_CONFIG_BYTES)?;
        let mut value: serde_json::Value = serde_json::from_slice(&bytes)?;
        let schema = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if schema == 0 {
            value["schema_version"] = serde_json::json!(INSTALL_CONFIG_VERSION);
            if value.get("preserve_state_on_uninstall").is_none() {
                value["preserve_state_on_uninstall"] = serde_json::json!(true);
            }
        }
        let config: Self = serde_json::from_value(value)?;
        config.validate()?;
        Ok(config)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallState {
    pub schema_version: u16,
    pub current_version: String,
    pub previous_version: Option<String>,
    pub current_binary_sha256: String,
    pub previous_binary_sha256: Option<String>,
}

impl InstallState {
    pub fn fresh(version: &str, binary_sha256: String) -> Result<Self, InstallError> {
        Version::parse(version)?;
        validate_sha256(&binary_sha256)?;
        Ok(Self {
            schema_version: INSTALL_STATE_VERSION,
            current_version: version.to_owned(),
            previous_version: None,
            current_binary_sha256: binary_sha256,
            previous_binary_sha256: None,
        })
    }

    pub fn validate(&self) -> Result<(), InstallError> {
        if self.schema_version != INSTALL_STATE_VERSION {
            return Err(InstallError::UnsupportedStateVersion(self.schema_version));
        }
        Version::parse(&self.current_version)?;
        validate_sha256(&self.current_binary_sha256)?;
        match (&self.previous_version, &self.previous_binary_sha256) {
            (Some(version), Some(checksum)) => {
                Version::parse(version)?;
                validate_sha256(checksum)?;
            }
            (None, None) => {}
            _ => return Err(InstallError::InvalidRollbackState),
        }
        Ok(())
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), InstallError> {
        self.validate()?;
        atomic_write_json(path.as_ref(), self)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, InstallError> {
        let bytes = read_bounded(path.as_ref(), MAX_CONFIG_BYTES)?;
        let state: Self = serde_json::from_slice(&bytes)?;
        state.validate()?;
        Ok(state)
    }

    pub fn plan_upgrade(
        &self,
        manifest: &ReleaseManifest,
        layout: &InstallLayout,
    ) -> Result<UpgradePlan, InstallError> {
        self.validate()?;
        manifest.validate()?;
        let current = Version::parse(&self.current_version)?;
        let target = Version::parse(&manifest.version)?;
        if target <= current {
            return Err(InstallError::UpgradeNotNewer {
                current: self.current_version.clone(),
                target: manifest.version.clone(),
            });
        }
        Ok(UpgradePlan {
            from_version: self.current_version.clone(),
            to_version: manifest.version.clone(),
            backup_binary: layout
                .backup_root
                .join(format!("dragonforge-test-lab-{}", self.current_version)),
            backup_state: layout
                .backup_root
                .join(format!("install-state-{}.json", self.current_version)),
        })
    }

    pub fn upgraded(
        &self,
        manifest: &ReleaseManifest,
    ) -> Result<InstallState, InstallError> {
        self.plan_upgrade(
            manifest,
            &InstallLayout::fixture(".", InstallPlatform::Linux),
        )?;
        Ok(InstallState {
            schema_version: INSTALL_STATE_VERSION,
            current_version: manifest.version.clone(),
            previous_version: Some(self.current_version.clone()),
            current_binary_sha256: manifest.binary_sha256.clone(),
            previous_binary_sha256: Some(self.current_binary_sha256.clone()),
        })
    }

    pub fn rollback_target(&self) -> Result<RollbackTarget, InstallError> {
        self.validate()?;
        Ok(RollbackTarget {
            version: self
                .previous_version
                .clone()
                .ok_or(InstallError::RollbackUnavailable)?,
            binary_sha256: self
                .previous_binary_sha256
                .clone()
                .ok_or(InstallError::RollbackUnavailable)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpgradePlan {
    pub from_version: String,
    pub to_version: String,
    pub backup_binary: PathBuf,
    pub backup_state: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackTarget {
    pub version: String,
    pub binary_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

impl Version {
    fn parse(value: &str) -> Result<Self, InstallError> {
        if value.is_empty() || value.len() > 64 {
            return Err(InstallError::InvalidVersion(value.to_owned()));
        }
        if value.contains('-') || value.contains('+') {
            return Err(InstallError::InvalidVersion(value.to_owned()));
        }
        let mut parts = value.split('.');
        let major = parse_version_part(parts.next(), value)?;
        let minor = parse_version_part(parts.next(), value)?;
        let patch = parse_version_part(parts.next(), value)?;
        if parts.next().is_some() {
            return Err(InstallError::InvalidVersion(value.to_owned()));
        }
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

pub fn sha256_file(path: &Path, max_bytes: u64) -> Result<String, InstallError> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > max_bytes {
            return Err(InstallError::InputTooLarge);
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn parse_version_part(part: Option<&str>, original: &str) -> Result<u64, InstallError> {
    let part = part.ok_or_else(|| InstallError::InvalidVersion(original.to_owned()))?;
    if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(InstallError::InvalidVersion(original.to_owned()));
    }
    part.parse()
        .map_err(|_| InstallError::InvalidVersion(original.to_owned()))
}

fn validate_token(value: &str, max_len: usize) -> Result<(), InstallError> {
    if value.is_empty()
        || value.len() > max_len
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(InstallError::InvalidToken);
    }
    Ok(())
}

fn validate_leaf_name(value: &str) -> Result<(), InstallError> {
    if value.is_empty()
        || value.len() > 255
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.contains('\0')
        || value.contains('\r')
        || value.contains('\n')
    {
        return Err(InstallError::InvalidBinaryName);
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), InstallError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(InstallError::InvalidChecksum);
    }
    Ok(())
}

fn validate_managed_path(path: &Path) -> Result<(), InstallError> {
    let value = path.to_string_lossy();
    if value.is_empty()
        || value.len() > 2048
        || value.contains('\0')
        || value.contains('\r')
        || value.contains('\n')
    {
        return Err(InstallError::InvalidPath);
    }
    Ok(())
}

fn read_bounded(path: &Path, max_bytes: u64) -> Result<Vec<u8>, InstallError> {
    let mut reader = File::open(path)?.take(max_bytes + 1);
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(InstallError::InputTooLarge);
    }
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        bytes.drain(..3);
    }
    Ok(bytes)
}

fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), InstallError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("tmp");
    let bytes = serde_json::to_vec_pretty(value)?;
    {
        let mut file = File::create(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(&temporary, path)?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("unsupported installer platform: {0}")]
    UnsupportedPlatform(String),
    #[error("unsupported release manifest schema version: {0}")]
    UnsupportedManifestVersion(u16),
    #[error("unsupported install state schema version: {0}")]
    UnsupportedStateVersion(u16),
    #[error("unsupported managed install config schema version: {0}")]
    UnsupportedConfigVersion(u16),
    #[error("invalid semantic version: {0}")]
    InvalidVersion(String),
    #[error("manifest token is invalid")]
    InvalidToken,
    #[error("manifest binary filename must be a safe leaf name")]
    InvalidBinaryName,
    #[error("SHA-256 checksum must contain exactly 64 hexadecimal characters")]
    InvalidChecksum,
    #[error("managed install path is invalid")]
    InvalidPath,
    #[error("installer input exceeded its bounded size")]
    InputTooLarge,
    #[error("release target mismatch: manifest={manifest_os}/{manifest_arch}, expected={expected_os}/{expected_arch}")]
    TargetMismatch {
        manifest_os: String,
        manifest_arch: String,
        expected_os: String,
        expected_arch: String,
    },
    #[error("release binary is missing: {0}")]
    BinaryMissing(PathBuf),
    #[error("release binary SHA-256 checksum does not match the manifest")]
    ChecksumMismatch,
    #[error("release binary must not be a symbolic link")]
    SymlinkedBinaryForbidden,
    #[error("upgrade target {target} must be newer than installed version {current}")]
    UpgradeNotNewer { current: String, target: String },
    #[error("install state contains incomplete rollback metadata")]
    InvalidRollbackState,
    #[error("no previous installation is available for rollback")]
    RollbackUnavailable,
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "dragonforge-phase21-{name}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn layouts_are_stable_and_platform_specific() {
        let linux = InstallLayout::production(InstallPlatform::Linux);
        assert_eq!(
            linux.binary_path,
            PathBuf::from("/opt/dragonforge/test-lab/bin/dragonforge-test-lab")
        );
        assert_eq!(
            linux.install_state_path,
            PathBuf::from("/var/lib/dragonforge/test-lab/install-state.json")
        );

        let windows = InstallLayout::production(InstallPlatform::Windows);
        assert!(windows
            .binary_path
            .to_string_lossy()
            .contains("Program Files"));
        assert_eq!(windows.service_name, "DragonForgeTestWorker");
    }

    #[test]
    fn manifest_verifies_binary_and_rejects_wrong_target() {
        let root = fixture_root("manifest");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let binary = root.join("dragonforge-test-lab");
        fs::write(&binary, b"phase21-binary").unwrap();
        let checksum = sha256_file(&binary, MAX_BINARY_BYTES).unwrap();
        let manifest = ReleaseManifest {
            schema_version: INSTALL_MANIFEST_VERSION,
            version: "0.22.0".into(),
            target_os: "linux".into(),
            target_arch: "x86_64".into(),
            binary_file: "dragonforge-test-lab".into(),
            binary_sha256: checksum,
        };
        assert_eq!(
            manifest
                .verify_package(&root, "linux", "x86_64")
                .unwrap(),
            binary
        );
        assert!(manifest
            .verify_package(&root, "windows", "x86_64")
            .is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn upgrade_and_rollback_metadata_are_bounded_to_previous_version() {
        let current = InstallState::fresh(
            "0.21.0",
            "11".repeat(32),
        )
        .unwrap();
        let manifest = ReleaseManifest {
            schema_version: INSTALL_MANIFEST_VERSION,
            version: "0.22.0".into(),
            target_os: "linux".into(),
            target_arch: "x86_64".into(),
            binary_file: "dragonforge-test-lab".into(),
            binary_sha256: "22".repeat(32),
        };
        let layout = InstallLayout::fixture(fixture_root("upgrade"), InstallPlatform::Linux);
        let plan = current.plan_upgrade(&manifest, &layout).unwrap();
        assert_eq!(plan.from_version, "0.21.0");
        assert_eq!(plan.to_version, "0.22.0");

        let upgraded = current.upgraded(&manifest).unwrap();
        let rollback = upgraded.rollback_target().unwrap();
        assert_eq!(rollback.version, "0.21.0");
        assert_eq!(rollback.binary_sha256, "11".repeat(32));
    }

    #[test]
    fn legacy_managed_config_migrates_to_schema_one() {
        let root = fixture_root("config");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("install-config.json");
        fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({
                "config_root": root.join("config"),
                "state_root": root.join("state"),
                "log_root": root.join("logs")
            }))
            .unwrap(),
        )
        .unwrap();

        let config = ManagedInstallConfig::load(&path).unwrap();
        assert_eq!(config.schema_version, INSTALL_CONFIG_VERSION);
        assert!(config.preserve_state_on_uninstall);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn package_binary_name_cannot_escape_package_root() {
        let manifest = ReleaseManifest {
            schema_version: INSTALL_MANIFEST_VERSION,
            version: "0.22.0".into(),
            target_os: "linux".into(),
            target_arch: "x86_64".into(),
            binary_file: "../dragonforge-test-lab".into(),
            binary_sha256: "00".repeat(32),
        };
        assert!(matches!(
            manifest.validate(),
            Err(InstallError::InvalidBinaryName)
        ));
    }
}
