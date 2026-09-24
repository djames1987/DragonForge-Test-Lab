use df_test_github::GitHubRepository;
use df_test_protocol::{JobRequest, RepositorySpec, ResourceLimits, TestAction};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;

pub const DOGFOOD_SCHEMA_VERSION: u16 = 1;
pub const MAX_DOGFOOD_PROFILES: usize = 64;
pub const MAX_MANUAL_ITEMS: usize = 32;
pub const MAX_PROFILE_BYTES: usize = 1024 * 1024;
pub const MAX_DOGFOOD_DEPTH: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DogfoodValidationProfile {
    RustFast,
    RustStandard,
    RustRelease,
}

impl DogfoodValidationProfile {
    pub fn actions(self) -> Vec<TestAction> {
        match self {
            Self::RustFast => vec![
                TestAction::Checkout,
                TestAction::CargoFmtCheck,
                TestAction::CargoTest { all_features: false },
            ],
            Self::RustStandard => vec![
                TestAction::Checkout,
                TestAction::CargoFmtCheck,
                TestAction::CargoClippy { deny_warnings: true },
                TestAction::CargoTest { all_features: true },
            ],
            Self::RustRelease => vec![
                TestAction::Checkout,
                TestAction::CargoBuild { release: true },
                TestAction::CargoTest { all_features: true },
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DogfoodProfile {
    pub schema_version: u16,
    pub name: String,
    pub repository: String,
    pub default_revision: String,
    pub validation_profile: DogfoodValidationProfile,
    pub enabled: bool,
    pub self_hosted: bool,
    pub max_depth: u8,
    #[serde(default)]
    pub manual_validation: Vec<String>,
    #[serde(default)]
    pub limits: ResourceLimits,
}

impl DogfoodProfile {
    pub fn validate(&self) -> Result<(), DogfoodError> {
        if self.schema_version != DOGFOOD_SCHEMA_VERSION {
            return Err(DogfoodError::UnsupportedSchema(self.schema_version));
        }
        validate_name(&self.name)?;
        GitHubRepository::parse_https(&self.repository)?;
        validate_revision(&self.default_revision)?;
        if self.max_depth > MAX_DOGFOOD_DEPTH {
            return Err(DogfoodError::InvalidDepth);
        }
        if self.self_hosted && self.max_depth != 1 {
            return Err(DogfoodError::SelfHostedDepthMustBeOne);
        }
        if self.manual_validation.len() > MAX_MANUAL_ITEMS {
            return Err(DogfoodError::TooManyManualItems);
        }
        for item in &self.manual_validation {
            if item.is_empty()
                || item.len() > 256
                || item.chars().any(|ch| matches!(ch, '\r' | '\n' | '\0'))
            {
                return Err(DogfoodError::InvalidManualItem);
            }
        }
        validate_limits(&self.limits)?;
        Ok(())
    }

    pub fn compile_job(&self, immutable_sha: &str, depth: u8) -> Result<JobRequest, DogfoodError> {
        self.validate()?;
        validate_sha(immutable_sha)?;
        if depth > self.max_depth || depth > MAX_DOGFOOD_DEPTH {
            return Err(DogfoodError::RecursionBlocked);
        }
        if self.self_hosted && depth != 1 {
            return Err(DogfoodError::SelfHostedDepthRequired);
        }

        let mut job = JobRequest::new(
            RepositorySpec {
                url: self.repository.clone(),
                revision: immutable_sha.to_owned(),
            },
            self.validation_profile.actions(),
        );
        job.limits = self.limits.clone();
        Ok(job)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DogfoodCampaign {
    pub schema_version: u16,
    pub name: String,
    pub profiles: Vec<DogfoodProfile>,
}

impl DogfoodCampaign {
    pub fn validate(&self) -> Result<(), DogfoodError> {
        if self.schema_version != DOGFOOD_SCHEMA_VERSION {
            return Err(DogfoodError::UnsupportedSchema(self.schema_version));
        }
        validate_name(&self.name)?;
        if self.profiles.is_empty() || self.profiles.len() > MAX_DOGFOOD_PROFILES {
            return Err(DogfoodError::InvalidProfileCount);
        }

        let mut names = BTreeSet::new();
        let mut repositories = BTreeSet::new();
        for profile in &self.profiles {
            profile.validate()?;
            if !names.insert(profile.name.clone()) {
                return Err(DogfoodError::DuplicateProfileName);
            }
            let normalized = profile.repository.trim_end_matches(".git").to_ascii_lowercase();
            if !repositories.insert(normalized) {
                return Err(DogfoodError::DuplicateRepository);
            }
        }
        Ok(())
    }

    pub fn enabled_profiles(&self) -> impl Iterator<Item = &DogfoodProfile> {
        self.profiles.iter().filter(|profile| profile.enabled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DogfoodRunRecord {
    pub profile: String,
    pub repository: String,
    pub immutable_sha: String,
    pub status: String,
    pub summary: String,
    pub artifact_directory: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DogfoodCampaignReport {
    pub schema_version: u16,
    pub campaign: String,
    pub runs: Vec<DogfoodRunRecord>,
}

impl DogfoodCampaignReport {
    pub fn passed(&self) -> bool {
        !self.runs.is_empty() && self.runs.iter().all(|run| run.status == "passed")
    }
}

pub fn load_profile(bytes: &[u8]) -> Result<DogfoodProfile, DogfoodError> {
    if bytes.is_empty() || bytes.len() > MAX_PROFILE_BYTES {
        return Err(DogfoodError::InvalidInputSize);
    }
    let profile: DogfoodProfile = serde_json::from_slice(bytes)?;
    profile.validate()?;
    Ok(profile)
}

pub fn load_campaign(bytes: &[u8]) -> Result<DogfoodCampaign, DogfoodError> {
    if bytes.is_empty() || bytes.len() > MAX_PROFILE_BYTES {
        return Err(DogfoodError::InvalidInputSize);
    }
    let campaign: DogfoodCampaign = serde_json::from_slice(bytes)?;
    campaign.validate()?;
    Ok(campaign)
}

fn validate_name(value: &str) -> Result<(), DogfoodError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(DogfoodError::InvalidName);
    }
    Ok(())
}

fn validate_revision(value: &str) -> Result<(), DogfoodError> {
    if value.is_empty()
        || value.len() > 256
        || value.starts_with('-')
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '/' | '-'))
    {
        return Err(DogfoodError::InvalidRevision);
    }
    Ok(())
}

fn validate_sha(value: &str) -> Result<(), DogfoodError> {
    if value.len() != 40 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(DogfoodError::InvalidCommitSha);
    }
    Ok(())
}

fn validate_limits(limits: &ResourceLimits) -> Result<(), DogfoodError> {
    if !(1..=86_400).contains(&limits.timeout_seconds)
        || !(128..=131_072).contains(&limits.max_memory_mib)
        || !(128..=1_048_576).contains(&limits.max_disk_mib)
        || !(1..=4_096).contains(&limits.max_processes)
    {
        return Err(DogfoodError::InvalidLimits);
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum DogfoodError {
    #[error("unsupported dogfood schema version {0}")]
    UnsupportedSchema(u16),
    #[error("invalid dogfood profile/campaign name")]
    InvalidName,
    #[error("invalid revision")]
    InvalidRevision,
    #[error("invalid immutable commit sha")]
    InvalidCommitSha,
    #[error("dogfood recursion depth is invalid")]
    InvalidDepth,
    #[error("self-hosted profiles must use max_depth=1")]
    SelfHostedDepthMustBeOne,
    #[error("self-hosted dogfood execution requires depth=1")]
    SelfHostedDepthRequired,
    #[error("dogfood recursion blocked")]
    RecursionBlocked,
    #[error("dogfood campaign profile count is invalid")]
    InvalidProfileCount,
    #[error("duplicate dogfood profile name")]
    DuplicateProfileName,
    #[error("duplicate dogfood repository")]
    DuplicateRepository,
    #[error("too many manual-validation entries")]
    TooManyManualItems,
    #[error("invalid manual-validation entry")]
    InvalidManualItem,
    #[error("invalid resource limits")]
    InvalidLimits,
    #[error("dogfood input must be between 1 byte and 1 MiB")]
    InvalidInputSize,
    #[error(transparent)]
    GitHub(#[from] df_test_github::GitHubError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(self_hosted: bool) -> DogfoodProfile {
        DogfoodProfile {
            schema_version: DOGFOOD_SCHEMA_VERSION,
            name: if self_hosted {
                "test-lab".into()
            } else {
                "security-suite".into()
            },
            repository: if self_hosted {
                "https://github.com/djames1987/DragonForge-Test-Lab.git".into()
            } else {
                "https://github.com/djames1987/DragonForge-Security-Suite.git".into()
            },
            default_revision: "main".into(),
            validation_profile: DogfoodValidationProfile::RustStandard,
            enabled: true,
            self_hosted,
            max_depth: if self_hosted { 1 } else { 0 },
            manual_validation: vec![],
            limits: ResourceLimits::default(),
        }
    }

    #[test]
    fn self_hosting_requires_exactly_one_orchestration_layer() {
        let p = profile(true);
        let sha = "a".repeat(40);
        assert!(p.compile_job(&sha, 0).is_err());
        assert!(p.compile_job(&sha, 1).is_ok());
        assert!(p.compile_job(&sha, 2).is_err());
    }

    #[test]
    fn external_profile_cannot_recurse() {
        let p = profile(false);
        let sha = "b".repeat(40);
        assert!(p.compile_job(&sha, 0).is_ok());
        assert!(p.compile_job(&sha, 1).is_err());
    }

    #[test]
    fn profiles_compile_only_typed_actions() {
        let p = profile(false);
        let job = p.compile_job(&"c".repeat(40), 0).unwrap();
        assert_eq!(
            job.actions,
            vec![
                TestAction::Checkout,
                TestAction::CargoFmtCheck,
                TestAction::CargoClippy { deny_warnings: true },
                TestAction::CargoTest { all_features: true }
            ]
        );
    }

    #[test]
    fn campaign_rejects_duplicate_repository() {
        let a = profile(false);
        let mut b = a.clone();
        b.name = "duplicate".into();
        let campaign = DogfoodCampaign {
            schema_version: DOGFOOD_SCHEMA_VERSION,
            name: "phase25".into(),
            profiles: vec![a, b],
        };
        assert!(matches!(
            campaign.validate(),
            Err(DogfoodError::DuplicateRepository)
        ));
    }

    #[test]
    fn profile_input_is_bounded() {
        assert!(matches!(
            load_profile(&vec![b'x'; MAX_PROFILE_BYTES + 1]),
            Err(DogfoodError::InvalidInputSize)
        ));
    }
}
