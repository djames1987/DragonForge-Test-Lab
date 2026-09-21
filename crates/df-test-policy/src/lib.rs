use df_test_protocol::{Capability, JobRequest};
use std::collections::BTreeSet;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct ExecutionPolicy {
    allowed_repository_prefixes: Vec<String>,
    allowed_capabilities: BTreeSet<Capability>,
    max_timeout_seconds: u64,
    max_memory_mib: u64,
    max_disk_mib: u64,
    max_processes: u32,
}

impl ExecutionPolicy {
    pub fn new(
        allowed_repository_prefixes: Vec<String>,
        allowed_capabilities: BTreeSet<Capability>,
    ) -> Self {
        Self {
            allowed_repository_prefixes,
            allowed_capabilities,
            max_timeout_seconds: 3600,
            max_memory_mib: 16384,
            max_disk_mib: 32768,
            max_processes: 256,
        }
    }

    pub fn authorize(&self, job: &JobRequest) -> Result<(), PolicyError> {
        if !job.repository.url.starts_with("https://") {
            return Err(PolicyError::InsecureRepositoryUrl);
        }

        if !self
            .allowed_repository_prefixes
            .iter()
            .any(|prefix| job.repository.url.starts_with(prefix))
        {
            return Err(PolicyError::RepositoryNotAllowed);
        }

        for capability in job.required_capabilities() {
            if !self.allowed_capabilities.contains(&capability) {
                return Err(PolicyError::CapabilityNotAllowed(capability));
            }
        }

        if job.limits.timeout_seconds > self.max_timeout_seconds
            || job.limits.max_memory_mib > self.max_memory_mib
            || job.limits.max_disk_mib > self.max_disk_mib
            || job.limits.max_processes > self.max_processes
        {
            return Err(PolicyError::ResourceLimitExceeded);
        }

        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("repository URL must use HTTPS")]
    InsecureRepositoryUrl,
    #[error("repository is not allowlisted")]
    RepositoryNotAllowed,
    #[error("capability is not allowed: {0:?}")]
    CapabilityNotAllowed(Capability),
    #[error("requested resource limit exceeds worker policy")]
    ResourceLimitExceeded,
}

#[cfg(test)]
mod tests {
    use super::*;
    use df_test_protocol::{RepositorySpec, TestAction};

    fn policy() -> ExecutionPolicy {
        ExecutionPolicy::new(
            vec!["https://github.com/djames1987/".into()],
            [Capability::CheckoutRepository, Capability::CargoTest]
                .into_iter()
                .collect(),
        )
    }

    #[test]
    fn allows_allowlisted_typed_job() {
        let job = JobRequest::new(
            RepositorySpec {
                url: "https://github.com/djames1987/project.git".into(),
                revision: "abc123".into(),
            },
            vec![TestAction::Checkout, TestAction::CargoTest { all_features: false }],
        );
        assert_eq!(policy().authorize(&job), Ok(()));
    }

    #[test]
    fn rejects_non_allowlisted_repository() {
        let job = JobRequest::new(
            RepositorySpec {
                url: "https://github.com/untrusted/project.git".into(),
                revision: "main".into(),
            },
            vec![TestAction::Checkout],
        );
        assert_eq!(policy().authorize(&job), Err(PolicyError::RepositoryNotAllowed));
    }

    #[test]
    fn rejects_capability_escalation() {
        let job = JobRequest::new(
            RepositorySpec {
                url: "https://github.com/djames1987/project.git".into(),
                revision: "main".into(),
            },
            vec![TestAction::CargoBuild { release: true }],
        );
        assert_eq!(
            policy().authorize(&job),
            Err(PolicyError::CapabilityNotAllowed(Capability::CargoBuild))
        );
    }
}
