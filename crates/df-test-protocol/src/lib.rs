use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    CheckoutRepository,
    CargoBuild,
    CargoTest,
    CargoClippy,
    CargoFmtCheck,
    ReadArtifacts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositorySpec {
    pub url: String,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TestAction {
    Checkout,
    CargoBuild { release: bool },
    CargoTest { all_features: bool },
    CargoClippy { deny_warnings: bool },
    CargoFmtCheck,
}

impl TestAction {
    pub fn required_capability(&self) -> Capability {
        match self {
            Self::Checkout => Capability::CheckoutRepository,
            Self::CargoBuild { .. } => Capability::CargoBuild,
            Self::CargoTest { .. } => Capability::CargoTest,
            Self::CargoClippy { .. } => Capability::CargoClippy,
            Self::CargoFmtCheck => Capability::CargoFmtCheck,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub timeout_seconds: u64,
    pub max_memory_mib: u64,
    pub max_disk_mib: u64,
    pub max_processes: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            timeout_seconds: 900,
            max_memory_mib: 4096,
            max_disk_mib: 8192,
            max_processes: 64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRequest {
    pub id: Uuid,
    pub repository: RepositorySpec,
    pub actions: Vec<TestAction>,
    #[serde(default)]
    pub limits: ResourceLimits,
}

impl JobRequest {
    pub fn new(repository: RepositorySpec, actions: Vec<TestAction>) -> Self {
        Self {
            id: Uuid::new_v4(),
            repository,
            actions,
            limits: ResourceLimits::default(),
        }
    }

    pub fn required_capabilities(&self) -> BTreeSet<Capability> {
        self.actions
            .iter()
            .map(TestAction::required_capability)
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Assigned,
    Running,
    Passed,
    Failed,
    Rejected,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub name: String,
    pub relative_path: String,
    pub size_bytes: u64,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobResult {
    pub job_id: Uuid,
    pub status: JobStatus,
    pub summary: String,
    pub artifacts: Vec<ArtifactRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerRegistration {
    pub worker_id: String,
    pub protocol_version: u16,
    pub os: String,
    pub arch: String,
    pub capabilities: BTreeSet<Capability>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_are_derived_from_typed_actions() {
        let job = JobRequest::new(
            RepositorySpec {
                url: "https://github.com/example/project.git".into(),
                revision: "main".into(),
            },
            vec![TestAction::Checkout, TestAction::CargoTest { all_features: true }],
        );

        assert!(job.required_capabilities().contains(&Capability::CheckoutRepository));
        assert!(job.required_capabilities().contains(&Capability::CargoTest));
        assert_eq!(job.required_capabilities().len(), 2);
    }
}
