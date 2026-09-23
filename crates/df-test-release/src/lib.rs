use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
use thiserror::Error;

pub const RELEASE_BUNDLE_SCHEMA_VERSION: u16 = 1;
pub const MAX_RELEASE_ARTIFACTS: usize = 64;
pub const MAX_RELEASE_FILE_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseChannel {
    Dev,
    Beta,
    Stable,
}

impl ReleaseChannel {
    pub fn parse(value: &str) -> Result<Self, ReleaseError> {
        match value {
            "dev" => Ok(Self::Dev),
            "beta" => Ok(Self::Beta),
            "stable" => Ok(Self::Stable),
            _ => Err(ReleaseError::InvalidChannel),
        }
    }

    pub fn requires_signatures(self) -> bool {
        matches!(self, Self::Stable)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Beta => "beta",
            Self::Stable => "stable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseVersion {
    pub base: String,
    pub channel: ReleaseChannel,
    pub iteration: Option<u32>,
}

impl ReleaseVersion {
    pub fn parse(value: &str, channel: ReleaseChannel) -> Result<Self, ReleaseError> {
        let (base, iteration) = match channel {
            ReleaseChannel::Stable => (value, None),
            ReleaseChannel::Beta => parse_qualified(value, "-beta.")?,
            ReleaseChannel::Dev => parse_qualified(value, "-dev.")?,
        };
        validate_base_version(base)?;
        Ok(Self {
            base: base.to_owned(),
            channel,
            iteration,
        })
    }

    pub fn tag(&self) -> String {
        match (self.channel, self.iteration) {
            (ReleaseChannel::Stable, None) => format!("v{}", self.base),
            (ReleaseChannel::Beta, Some(n)) => format!("v{}-beta.{n}", self.base),
            (ReleaseChannel::Dev, Some(n)) => format!("v{}-dev.{n}", self.base),
            _ => unreachable!("validated release version"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseArtifactKind {
    WindowsPackage,
    LinuxPackage,
    Sbom,
    Checksums,
    AuditReport,
    ReleaseNotes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseArtifact {
    pub kind: ReleaseArtifactKind,
    pub file: String,
    pub sha256: String,
    pub signature_file: Option<String>,
}

impl ReleaseArtifact {
    pub fn validate(&self) -> Result<(), ReleaseError> {
        validate_leaf(&self.file)?;
        validate_sha256(&self.sha256)?;
        if let Some(signature) = &self.signature_file {
            validate_leaf(signature)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseBundleManifest {
    pub schema_version: u16,
    pub version: String,
    pub channel: ReleaseChannel,
    pub git_commit: String,
    pub artifacts: Vec<ReleaseArtifact>,
}

impl ReleaseBundleManifest {
    pub fn validate(&self) -> Result<(), ReleaseError> {
        if self.schema_version != RELEASE_BUNDLE_SCHEMA_VERSION {
            return Err(ReleaseError::UnsupportedSchema(self.schema_version));
        }
        ReleaseVersion::parse(&self.version, self.channel)?;
        validate_git_sha(&self.git_commit)?;
        if self.artifacts.is_empty() || self.artifacts.len() > MAX_RELEASE_ARTIFACTS {
            return Err(ReleaseError::InvalidArtifactCount);
        }
        for artifact in &self.artifacts {
            artifact.validate()?;
            if self.channel.requires_signatures()
                && matches!(
                    artifact.kind,
                    ReleaseArtifactKind::WindowsPackage | ReleaseArtifactKind::LinuxPackage
                )
                && artifact.signature_file.is_none()
            {
                return Err(ReleaseError::StableArtifactUnsigned(artifact.file.clone()));
            }
        }
        Ok(())
    }

    pub fn verify_root(&self, root: impl AsRef<Path>) -> Result<(), ReleaseError> {
        self.validate()?;
        let root = root.as_ref();
        for artifact in &self.artifacts {
            let path = root.join(&artifact.file);
            if !path.is_file() {
                return Err(ReleaseError::MissingArtifact(path));
            }
            if sha256_file(&path)? != artifact.sha256 {
                return Err(ReleaseError::ChecksumMismatch(artifact.file.clone()));
            }
            if let Some(signature) = &artifact.signature_file {
                if !root.join(signature).is_file() {
                    return Err(ReleaseError::MissingSignature(signature.clone()));
                }
            }
        }
        Ok(())
    }
}

pub fn sha256_file(path: &Path) -> Result<String, ReleaseError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > MAX_RELEASE_FILE_BYTES {
            return Err(ReleaseError::InputTooLarge);
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn parse_qualified<'a>(
    value: &'a str,
    marker: &str,
) -> Result<(&'a str, Option<u32>), ReleaseError> {
    let (base, number) = value
        .split_once(marker)
        .ok_or(ReleaseError::InvalidVersion)?;
    if base.contains('-') || number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ReleaseError::InvalidVersion);
    }
    let iteration = number
        .parse::<u32>()
        .map_err(|_| ReleaseError::InvalidVersion)?;
    if iteration == 0 {
        return Err(ReleaseError::InvalidVersion);
    }
    Ok((base, Some(iteration)))
}

fn validate_base_version(value: &str) -> Result<(), ReleaseError> {
    if value.is_empty() || value.len() > 64 || value.contains('-') || value.contains('+') {
        return Err(ReleaseError::InvalidVersion);
    }
    let parts: Vec<&str> = value.split('.').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(ReleaseError::InvalidVersion);
    }
    Ok(())
}

fn validate_leaf(value: &str) -> Result<(), ReleaseError> {
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
        return Err(ReleaseError::InvalidArtifactName);
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), ReleaseError> {
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ReleaseError::InvalidChecksum);
    }
    Ok(())
}

fn validate_git_sha(value: &str) -> Result<(), ReleaseError> {
    if value.len() != 40 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ReleaseError::InvalidGitCommit);
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum ReleaseError {
    #[error("release channel must be dev, beta, or stable")]
    InvalidChannel,
    #[error("release version does not match the selected channel")]
    InvalidVersion,
    #[error("unsupported release bundle schema: {0}")]
    UnsupportedSchema(u16),
    #[error("release bundle must contain between 1 and 64 artifacts")]
    InvalidArtifactCount,
    #[error("release artifact filename must be a safe leaf name")]
    InvalidArtifactName,
    #[error("release artifact checksum must be SHA-256")]
    InvalidChecksum,
    #[error("release git commit must be a full 40-character SHA")]
    InvalidGitCommit,
    #[error("stable release artifact is unsigned: {0}")]
    StableArtifactUnsigned(String),
    #[error("release artifact is missing: {0}")]
    MissingArtifact(PathBuf),
    #[error("release signature is missing: {0}")]
    MissingSignature(String),
    #[error("release artifact checksum mismatch: {0}")]
    ChecksumMismatch(String),
    #[error("release input exceeded the maximum supported size")]
    InputTooLarge,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn release_channels_enforce_version_shapes() {
        assert_eq!(
            ReleaseVersion::parse("0.23.0", ReleaseChannel::Stable)
                .unwrap()
                .tag(),
            "v0.23.0"
        );
        assert_eq!(
            ReleaseVersion::parse("0.23.0-beta.2", ReleaseChannel::Beta)
                .unwrap()
                .tag(),
            "v0.23.0-beta.2"
        );
        assert!(ReleaseVersion::parse("0.23.0", ReleaseChannel::Beta).is_err());
        assert!(ReleaseVersion::parse("0.23.0-dev.0", ReleaseChannel::Dev).is_err());
    }

    #[test]
    fn stable_packages_require_signatures() {
        let manifest = ReleaseBundleManifest {
            schema_version: RELEASE_BUNDLE_SCHEMA_VERSION,
            version: "0.23.0".into(),
            channel: ReleaseChannel::Stable,
            git_commit: "a".repeat(40),
            artifacts: vec![ReleaseArtifact {
                kind: ReleaseArtifactKind::LinuxPackage,
                file: "linux.tar.gz".into(),
                sha256: "0".repeat(64),
                signature_file: None,
            }],
        };
        assert!(matches!(
            manifest.validate(),
            Err(ReleaseError::StableArtifactUnsigned(_))
        ));
    }

    #[test]
    fn beta_package_may_be_unsigned() {
        let manifest = ReleaseBundleManifest {
            schema_version: RELEASE_BUNDLE_SCHEMA_VERSION,
            version: "0.23.0-beta.1".into(),
            channel: ReleaseChannel::Beta,
            git_commit: "b".repeat(40),
            artifacts: vec![ReleaseArtifact {
                kind: ReleaseArtifactKind::WindowsPackage,
                file: "windows.zip".into(),
                sha256: "1".repeat(64),
                signature_file: None,
            }],
        };
        manifest.validate().unwrap();
    }

    #[test]
    fn fixture_verifies_checksums_and_signature_presence() {
        let root = std::env::temp_dir().join(format!(
            "dragonforge-phase22-release-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let package = root.join("package.zip");
        let signature = root.join("package.zip.sig");
        fs::write(&package, b"release-fixture").unwrap();
        fs::write(&signature, b"signature-fixture").unwrap();

        let manifest = ReleaseBundleManifest {
            schema_version: RELEASE_BUNDLE_SCHEMA_VERSION,
            version: "0.23.0".into(),
            channel: ReleaseChannel::Stable,
            git_commit: "c".repeat(40),
            artifacts: vec![ReleaseArtifact {
                kind: ReleaseArtifactKind::WindowsPackage,
                file: "package.zip".into(),
                sha256: sha256_file(&package).unwrap(),
                signature_file: Some("package.zip.sig".into()),
            }],
        };
        manifest.verify_root(&root).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
