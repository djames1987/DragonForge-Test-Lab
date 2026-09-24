use df_test_controller::{DurableController, DurableJobState};
use df_test_distributed::{
    AuthenticatedEnvelope, EnvelopeVerifier, MultiNodePlan, NodeFeature, NodeHeartbeat, NodeProfile,
    NodeRegistration, NodeRegistry, NodeRequirement,
};
use df_test_identity::IdentityTrustStore;
use df_test_lifecycle::RetryPolicy;
use df_test_protocol::{
    Capability, JobRequest, RepositorySpec, TestAction, WorkerRegistration, PROTOCOL_VERSION,
};
use df_test_worker_service::{WorkerServiceConfig, WorkerServiceRuntime, WorkerServiceState};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};
use thiserror::Error;
use uuid::Uuid;

pub const CHAOS_SCHEMA_VERSION: u16 = 1;
pub const DEFAULT_STRESS_JOBS: usize = 250;
pub const MAX_STRESS_JOBS: usize = 2_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChaosReport {
    pub schema_version: u16,
    pub controller_restart_recovered: bool,
    pub duplicate_assignment_prevented: bool,
    pub stale_node_excluded: bool,
    pub replay_rejected: bool,
    pub worker_restart_state_recovered: bool,
    pub worker_disconnect_backoff_bounded: bool,
    pub database_corruption_failed_closed: bool,
    pub disk_write_failure_failed_closed: bool,
    pub certificate_revocation_failed_closed: bool,
    pub stress_jobs_requested: usize,
    pub stress_jobs_completed: usize,
    pub audit_chain_valid: bool,
}

impl ChaosReport {
    pub fn passed(&self) -> bool {
        self.controller_restart_recovered
            && self.duplicate_assignment_prevented
            && self.stale_node_excluded
            && self.replay_rejected
            && self.worker_restart_state_recovered
            && self.worker_disconnect_backoff_bounded
            && self.database_corruption_failed_closed
            && self.disk_write_failure_failed_closed
            && self.certificate_revocation_failed_closed
            && self.stress_jobs_requested == self.stress_jobs_completed
            && self.audit_chain_valid
    }
}

pub fn run_chaos_fixture(stress_jobs: usize) -> Result<ChaosReport, ChaosError> {
    if !(1..=MAX_STRESS_JOBS).contains(&stress_jobs) {
        return Err(ChaosError::InvalidStressJobs);
    }

    let root = std::env::temp_dir().join(format!(
        "dragonforge-phase24-chaos-{}-{}",
        std::process::id(),
        Uuid::new_v4()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root)?;

    let controller_restart_recovered = controller_restart_scenario(&root)?;
    let duplicate_assignment_prevented = duplicate_assignment_scenario()?;
    let (stale_node_excluded, replay_rejected) = distributed_fault_scenario()?;
    let (worker_restart_state_recovered, worker_disconnect_backoff_bounded) =
        worker_restart_scenario(&root)?;
    let database_corruption_failed_closed = database_corruption_scenario(&root)?;
    let disk_write_failure_failed_closed = disk_fault_scenario(&root)?;
    let certificate_revocation_failed_closed = certificate_fault_scenario()?;
    let (stress_jobs_completed, audit_chain_valid) = stress_scenario(stress_jobs)?;

    let report = ChaosReport {
        schema_version: CHAOS_SCHEMA_VERSION,
        controller_restart_recovered,
        duplicate_assignment_prevented,
        stale_node_excluded,
        replay_rejected,
        worker_restart_state_recovered,
        worker_disconnect_backoff_bounded,
        database_corruption_failed_closed,
        disk_write_failure_failed_closed,
        certificate_revocation_failed_closed,
        stress_jobs_requested: stress_jobs,
        stress_jobs_completed,
        audit_chain_valid,
    };

    let _ = fs::remove_dir_all(root);
    Ok(report)
}

fn worker(id: &str) -> WorkerRegistration {
    WorkerRegistration {
        worker_id: id.into(),
        protocol_version: PROTOCOL_VERSION,
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        capabilities: [Capability::CheckoutRepository, Capability::CargoTest]
            .into_iter()
            .collect(),
    }
}

fn job() -> JobRequest {
    JobRequest::new(
        RepositorySpec {
            url: "https://github.com/djames1987/DragonForge-Test-Lab.git".into(),
            revision: "a".repeat(40),
        },
        vec![
            TestAction::Checkout,
            TestAction::CargoTest { all_features: true },
        ],
    )
}

fn controller_restart_scenario(root: &Path) -> Result<bool, ChaosError> {
    let db = root.join("restart.sqlite3");
    let request = job();
    {
        let mut controller = DurableController::open(&db)?;
        controller.enqueue_job_with_retry(
            &request,
            RetryPolicy::bounded(3, 5, 30, true)?,
            100,
        )?;
        controller.register_worker(&worker("chaos-worker-a"), 101)?;
        let assigned = controller
            .assign_next("chaos-worker-a", 102)?
            .ok_or(ChaosError::Scenario("restart job not assigned"))?;
        if assigned.id != request.id {
            return Ok(false);
        }
        controller.mark_running(request.id, 103)?;
    }

    let mut recovered = DurableController::open(&db)?;
    let interrupted = recovered.recover_after_restart(200)?;
    let record = recovered
        .get_job(request.id)?
        .ok_or(ChaosError::Scenario("restart job missing"))?;
    let attempts = recovered.list_attempts(request.id)?;
    Ok(interrupted == vec![request.id]
        && record.state == DurableJobState::RetryPending
        && record.assigned_worker.is_none()
        && attempts.len() == 1
        && attempts[0].state == DurableJobState::Interrupted)
}

fn duplicate_assignment_scenario() -> Result<bool, ChaosError> {
    let mut controller = DurableController::open_in_memory()?;
    let request = job();
    controller.enqueue_job(&request, 10)?;
    controller.register_worker(&worker("chaos-worker-a"), 11)?;
    controller.register_worker(&worker("chaos-worker-b"), 11)?;
    let first = controller.assign_next("chaos-worker-a", 12)?;
    let second = controller.assign_next("chaos-worker-b", 12)?;
    Ok(first.as_ref().map(|value| value.id) == Some(request.id) && second.is_none())
}

fn distributed_fault_scenario() -> Result<(bool, bool), ChaosError> {
    let secret = b"phase24-chaos-secret-material-012345";
    let mut verifier = EnvelopeVerifier::new(5)?;
    verifier.add_key("phase24-key", secret)?;
    let mut registry = NodeRegistry::new(verifier, 5)?;

    let registration = NodeRegistration {
        protocol_version: PROTOCOL_VERSION,
        profile: NodeProfile {
            node_id: "chaos-node".into(),
            os: "windows".into(),
            arch: "x86_64".into(),
            labels: BTreeSet::new(),
            features: [NodeFeature::FaultInjection].into_iter().collect(),
            max_parallel_jobs: 1,
        },
        outbound_only: true,
        key_id: "phase24-key".into(),
    };
    let envelope = AuthenticatedEnvelope::sign("phase24-key", secret, 100, registration)?;
    registry.register(&envelope, 100)?;

    let replay_rejected = registry.register(&envelope, 100).is_err();
    let stale_node_excluded = !registry.is_online("chaos-node", 106);

    let plan = MultiNodePlan {
        plan_id: Uuid::new_v4(),
        roles: vec![NodeRequirement {
            role: "faulted".into(),
            required_features: [NodeFeature::FaultInjection].into_iter().collect(),
            required_labels: BTreeSet::new(),
            os: Some("windows".into()),
            arch: None,
            max_load_percent: 100,
        }],
    };
    let allocation_rejected = registry.allocate_plan(&plan, 106).is_err();

    let heartbeat = NodeHeartbeat {
        node_id: "chaos-node".into(),
        load_percent: 10,
        active_jobs: 0,
    };
    let fresh = AuthenticatedEnvelope::sign("phase24-key", secret, 107, heartbeat)?;
    registry.heartbeat(&fresh, 107)?;

    Ok((
        stale_node_excluded && allocation_rejected && registry.is_online("chaos-node", 107),
        replay_rejected,
    ))
}

fn worker_restart_scenario(root: &Path) -> Result<(bool, bool), ChaosError> {
    let state_path = root.join("worker-state.json");
    let config = worker_config(state_path.clone());
    let mut runtime = WorkerServiceRuntime::new(config.clone())?;
    runtime.mark_connected(100);
    runtime.request_drain();
    runtime.persist()?;

    let recovered = WorkerServiceRuntime::recover(config)?;
    let restart_ok = recovered.snapshot().state == WorkerServiceState::Draining
        && recovered.snapshot().drain_requested
        && recovered.snapshot().last_heartbeat_secs.is_none()
        && !recovered.can_accept_job();

    let config = worker_config(root.join("worker-backoff.json"));
    let mut disconnected = WorkerServiceRuntime::new(config)?;
    for _ in 0..128 {
        disconnected.mark_disconnected();
    }
    let bounded = disconnected.reconnect_delay().as_secs() == 64;
    Ok((restart_ok, bounded))
}

fn worker_config(state_path: PathBuf) -> WorkerServiceConfig {
    WorkerServiceConfig {
        worker_id: "phase24-worker".into(),
        controller: "127.0.0.1:45891".parse::<SocketAddr>().unwrap(),
        controller_server_name: "localhost".into(),
        heartbeat_seconds: 10,
        max_parallel_jobs: 2,
        state_path: state_path.clone(),
        ca_cert_path: state_path.with_extension("ca.pem"),
        client_cert_path: state_path.with_extension("client.pem"),
        client_key_path: state_path.with_extension("key.pem"),
    }
}

fn database_corruption_scenario(root: &Path) -> Result<bool, ChaosError> {
    let path = root.join("corrupt.sqlite3");
    fs::write(&path, b"this is deliberately not a sqlite database")?;
    Ok(DurableController::open(&path).is_err())
}

fn disk_fault_scenario(root: &Path) -> Result<bool, ChaosError> {
    let blocker = root.join("not-a-directory");
    fs::write(&blocker, b"block parent directory creation")?;
    let config = worker_config(blocker.join("state.json"));
    let runtime = WorkerServiceRuntime::new(config)?;
    Ok(runtime.persist().is_err())
}

fn certificate_fault_scenario() -> Result<bool, ChaosError> {
    let cert = b"phase24-certificate";
    let mut trust = IdentityTrustStore::default();
    let enrolled = trust.enroll("chaos-node", cert, 10, 100)?;
    let before = trust.verify_peer("chaos-node", cert, 50).is_ok();
    trust.revoke_certificate(&enrolled.fingerprint)?;
    let after = trust.verify_peer("chaos-node", cert, 50).is_err();
    Ok(before && after)
}

fn stress_scenario(stress_jobs: usize) -> Result<(usize, bool), ChaosError> {
    let mut controller = DurableController::open_in_memory()?;
    controller.register_worker(&worker("stress-worker"), 1)?;
    let mut ids = Vec::with_capacity(stress_jobs);
    for index in 0..stress_jobs {
        let request = job();
        ids.push(request.id);
        controller.enqueue_job(&request, 10 + index as u64)?;
    }

    let mut completed = 0usize;
    for index in 0..stress_jobs {
        let assigned = controller
            .assign_next("stress-worker", 10_000 + index as u64)?
            .ok_or(ChaosError::Scenario("stress queue unexpectedly empty"))?;
        controller.mark_running(assigned.id, 20_000 + index as u64)?;
        controller.complete_job(
            &df_test_protocol::JobResult {
                job_id: assigned.id,
                status: df_test_protocol::JobStatus::Passed,
                summary: "chaos stress pass".into(),
                artifacts: vec![],
            },
            30_000 + index as u64,
        )?;
        completed += 1;
    }

    let no_duplicate_attempts = ids.iter().all(|id| {
        controller
            .list_attempts(*id)
            .map(|attempts| attempts.len() == 1)
            .unwrap_or(false)
    });
    Ok((
        if no_duplicate_attempts { completed } else { 0 },
        controller.verify_audit_chain()?,
    ))
}

#[derive(Debug, Error)]
pub enum ChaosError {
    #[error("stress job count must be between 1 and {MAX_STRESS_JOBS}")]
    InvalidStressJobs,
    #[error("chaos scenario failed: {0}")]
    Scenario(&'static str),
    #[error(transparent)]
    Controller(#[from] df_test_controller::DurableControllerError),
    #[error(transparent)]
    Distributed(#[from] df_test_distributed::DistributedError),
    #[error(transparent)]
    Identity(#[from] df_test_identity::IdentityError),
    #[error(transparent)]
    Lifecycle(#[from] df_test_lifecycle::LifecycleError),
    #[error(transparent)]
    Worker(#[from] df_test_worker_service::WorkerServiceError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_chaos_fixture_passes() {
        let report = run_chaos_fixture(50).unwrap();
        assert!(report.passed(), "{report:#?}");
    }

    #[test]
    fn stress_bounds_are_enforced() {
        assert!(matches!(
            run_chaos_fixture(0),
            Err(ChaosError::InvalidStressJobs)
        ));
        assert!(matches!(
            run_chaos_fixture(MAX_STRESS_JOBS + 1),
            Err(ChaosError::InvalidStressJobs)
        ));
    }
}
