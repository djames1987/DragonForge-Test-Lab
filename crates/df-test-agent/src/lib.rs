use df_test_policy::{ExecutionPolicy, PolicyError};
use df_test_protocol::{JobRequest, WorkerRegistration, PROTOCOL_VERSION};
use thiserror::Error;

pub struct Agent {
    registration: WorkerRegistration,
    policy: ExecutionPolicy,
}

impl Agent {
    pub fn new(registration: WorkerRegistration, policy: ExecutionPolicy) -> Result<Self, AgentError> {
        if registration.protocol_version != PROTOCOL_VERSION {
            return Err(AgentError::ProtocolMismatch {
                expected: PROTOCOL_VERSION,
                actual: registration.protocol_version,
            });
        }
        Ok(Self { registration, policy })
    }

    pub fn registration(&self) -> &WorkerRegistration {
        &self.registration
    }

    pub fn validate_job(&self, job: &JobRequest) -> Result<(), AgentError> {
        self.policy.authorize(job)?;
        let required = job.required_capabilities();
        if !required.is_subset(&self.registration.capabilities) {
            return Err(AgentError::WorkerCapabilityMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("protocol mismatch: expected {expected}, got {actual}")]
    ProtocolMismatch { expected: u16, actual: u16 },
    #[error("job denied by execution policy: {0}")]
    Policy(#[from] PolicyError),
    #[error("worker does not advertise all capabilities required by job")]
    WorkerCapabilityMismatch,
}
