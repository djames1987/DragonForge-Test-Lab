use df_test_intelligence::{
    ChangeSet, HistoricalFailure, TestProfile, WorkerCapacity, MAX_HISTORY_RECORDS,
};
use df_test_lifecycle::{decide_failure, FailureClass, LifecycleDecision, RetryPolicy};
use df_test_observability::{next_audit_digest, LogLevel, MetricPoint, StructuredLogEvent};
use df_test_plans::TestPlan;
use df_test_protocol::{ArtifactRef, JobRequest, JobResult, WorkerRegistration, PROTOCOL_VERSION};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    path::Path,
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct Controller {
    queued: VecDeque<JobRequest>,
    workers: HashMap<String, WorkerRegistration>,
}

impl Controller {
    pub fn register_worker(&mut self, worker: WorkerRegistration) -> Result<(), ControllerError> {
        if worker.protocol_version != PROTOCOL_VERSION {
            return Err(ControllerError::ProtocolMismatch);
        }
        self.workers.insert(worker.worker_id.clone(), worker);
        Ok(())
    }

    pub fn enqueue(&mut self, job: JobRequest) {
        self.queued.push_back(job);
    }

    pub fn queued_jobs(&self) -> usize {
        self.queued.len()
    }

    pub fn assign_next(&mut self, worker_id: &str) -> Result<Option<JobRequest>, ControllerError> {
        let worker = self
            .workers
            .get(worker_id)
            .ok_or(ControllerError::UnknownWorker)?;

        let position = self
            .queued
            .iter()
            .position(|job| job.required_capabilities().is_subset(&worker.capabilities));

        Ok(position.and_then(|index| self.queued.remove(index)))
    }

    pub fn cancel(&mut self, job_id: Uuid) -> bool {
        if let Some(index) = self.queued.iter().position(|job| job.id == job_id) {
            self.queued.remove(index);
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerError {
    ProtocolMismatch,
    UnknownWorker,
}

pub const SCHEMA_VERSION: i64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurableJobState {
    Queued,
    Assigned,
    Running,
    Passed,
    Failed,
    Rejected,
    Cancelled,
    Interrupted,
    RetryPending,
    Exhausted,
}

impl DurableJobState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Assigned => "assigned",
            Self::Running => "running",
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
            Self::RetryPending => "retry_pending",
            Self::Exhausted => "exhausted",
        }
    }

    fn parse(value: &str) -> Result<Self, DurableControllerError> {
        match value {
            "queued" => Ok(Self::Queued),
            "assigned" => Ok(Self::Assigned),
            "running" => Ok(Self::Running),
            "passed" => Ok(Self::Passed),
            "failed" => Ok(Self::Failed),
            "rejected" => Ok(Self::Rejected),
            "cancelled" => Ok(Self::Cancelled),
            "interrupted" => Ok(Self::Interrupted),
            "retry_pending" => Ok(Self::RetryPending),
            "exhausted" => Ok(Self::Exhausted),
            _ => Err(DurableControllerError::InvalidStoredState(value.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableJobRecord {
    pub job: JobRequest,
    pub state: DurableJobState,
    pub assigned_worker: Option<String>,
    pub created_at_secs: u64,
    pub updated_at_secs: u64,
    pub last_error: Option<String>,
    pub retry_policy: RetryPolicy,
    pub failure_class: Option<FailureClass>,
    pub next_retry_at_secs: Option<u64>,
    pub retry_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableAttemptRecord {
    pub attempt_id: i64,
    pub job_id: Uuid,
    pub attempt_number: u32,
    pub worker_id: Option<String>,
    pub state: DurableJobState,
    pub started_at_secs: Option<u64>,
    pub finished_at_secs: Option<u64>,
    pub summary: Option<String>,
    pub failure_class: Option<FailureClass>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableWorkerRecord {
    pub registration: WorkerRegistration,
    pub last_seen_secs: u64,
    pub online: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: i64,
    pub unix_time_secs: u64,
    pub kind: String,
    pub entity_type: String,
    pub entity_id: String,
    pub detail: serde_json::Value,
    pub previous_sha256: Option<String>,
    pub event_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DurableLogRecord {
    pub id: i64,
    pub event: StructuredLogEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DurableMetricRecord {
    pub id: i64,
    pub point: MetricPoint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableArtifactRecord {
    pub id: i64,
    pub job_id: Uuid,
    pub artifact: ArtifactRef,
    pub created_at_secs: u64,
    pub retained_until_secs: Option<u64>,
}

pub struct DurableController {
    connection: Connection,
}

impl DurableController {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DurableControllerError> {
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    pub fn open_in_memory() -> Result<Self, DurableControllerError> {
        let connection = Connection::open_in_memory()?;
        Self::from_connection(connection)
    }

    fn from_connection(connection: Connection) -> Result<Self, DurableControllerError> {
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;
        let mut controller = Self { connection };
        controller.migrate()?;
        Ok(controller)
    }

    pub fn schema_version(&self) -> Result<i64, DurableControllerError> {
        Ok(self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?)
    }

    pub fn migrate(&mut self) -> Result<(), DurableControllerError> {
        let current = self.schema_version()?;
        if current > SCHEMA_VERSION {
            return Err(DurableControllerError::SchemaTooNew {
                current,
                supported: SCHEMA_VERSION,
            });
        }

        if current < 1 {
            let tx = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS jobs (
                    job_id TEXT PRIMARY KEY NOT NULL,
                    request_json TEXT NOT NULL,
                    state TEXT NOT NULL,
                    assigned_worker TEXT,
                    created_at_secs INTEGER NOT NULL,
                    updated_at_secs INTEGER NOT NULL,
                    last_error TEXT
                );

                CREATE TABLE IF NOT EXISTS job_attempts (
                    attempt_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    job_id TEXT NOT NULL,
                    attempt_number INTEGER NOT NULL,
                    worker_id TEXT,
                    state TEXT NOT NULL,
                    started_at_secs INTEGER,
                    finished_at_secs INTEGER,
                    summary TEXT,
                    FOREIGN KEY(job_id) REFERENCES jobs(job_id) ON DELETE CASCADE,
                    UNIQUE(job_id, attempt_number)
                );

                CREATE TABLE IF NOT EXISTS workers (
                    worker_id TEXT PRIMARY KEY NOT NULL,
                    registration_json TEXT NOT NULL,
                    last_seen_secs INTEGER NOT NULL,
                    online INTEGER NOT NULL CHECK(online IN (0, 1))
                );

                CREATE TABLE IF NOT EXISTS intelligence_history (
                    intelligence_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    report_json TEXT NOT NULL,
                    created_at_secs INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS artifact_metadata (
                    artifact_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    job_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    relative_path TEXT NOT NULL,
                    size_bytes INTEGER NOT NULL,
                    sha256 TEXT,
                    FOREIGN KEY(job_id) REFERENCES jobs(job_id) ON DELETE CASCADE
                );

                CREATE TABLE IF NOT EXISTS audit_events (
                    audit_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    unix_time_secs INTEGER NOT NULL,
                    kind TEXT NOT NULL,
                    entity_type TEXT NOT NULL,
                    entity_id TEXT NOT NULL,
                    detail_json TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS controller_config (
                    config_key TEXT PRIMARY KEY NOT NULL,
                    config_value TEXT NOT NULL,
                    updated_at_secs INTEGER NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_jobs_state_created
                    ON jobs(state, created_at_secs);
                CREATE INDEX IF NOT EXISTS idx_attempts_job
                    ON job_attempts(job_id, attempt_number);
                CREATE INDEX IF NOT EXISTS idx_audit_entity
                    ON audit_events(entity_type, entity_id, unix_time_secs);

                PRAGMA user_version = 1;",
            )?;
            tx.commit()?;
        }

        if current < 2 {
            let tx = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute_batch(
                "ALTER TABLE artifact_metadata
                    ADD COLUMN created_at_secs INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE artifact_metadata
                    ADD COLUMN retained_until_secs INTEGER;
                 ALTER TABLE audit_events
                    ADD COLUMN previous_sha256 TEXT;
                 ALTER TABLE audit_events
                    ADD COLUMN event_sha256 TEXT;

                 CREATE TABLE IF NOT EXISTS structured_logs (
                    log_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    unix_time_secs INTEGER NOT NULL,
                    level TEXT NOT NULL,
                    component TEXT NOT NULL,
                    message TEXT NOT NULL,
                    fields_json TEXT NOT NULL,
                    job_id TEXT,
                    worker_id TEXT
                 );

                 CREATE TABLE IF NOT EXISTS metric_samples (
                    metric_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    unix_time_secs INTEGER NOT NULL,
                    name TEXT NOT NULL,
                    value REAL NOT NULL,
                    labels_json TEXT NOT NULL
                 );

                 CREATE INDEX IF NOT EXISTS idx_artifacts_created
                    ON artifact_metadata(created_at_secs, artifact_id);
                 CREATE INDEX IF NOT EXISTS idx_logs_time
                    ON structured_logs(unix_time_secs, log_id);
                 CREATE INDEX IF NOT EXISTS idx_logs_job
                    ON structured_logs(job_id, unix_time_secs);
                 CREATE INDEX IF NOT EXISTS idx_metrics_name_time
                    ON metric_samples(name, unix_time_secs);

                 PRAGMA user_version = 2;",
            )?;
            tx.commit()?;
        }

        if current < 3 {
            let tx = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute_batch(
                "ALTER TABLE jobs ADD COLUMN retry_policy_json TEXT;
                 ALTER TABLE jobs ADD COLUMN failure_class TEXT;
                 ALTER TABLE jobs ADD COLUMN next_retry_at_secs INTEGER;
                 ALTER TABLE jobs ADD COLUMN retry_reason TEXT;
                 ALTER TABLE job_attempts ADD COLUMN failure_class TEXT;

                 CREATE INDEX IF NOT EXISTS idx_jobs_retry_ready
                    ON jobs(state, next_retry_at_secs, created_at_secs);

                 PRAGMA user_version = 3;",
            )?;
            tx.commit()?;
        }

        if current < 4 {
            let tx = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS test_plans (
                    plan_name TEXT PRIMARY KEY NOT NULL,
                    plan_version INTEGER NOT NULL,
                    plan_json TEXT NOT NULL,
                    created_at_secs INTEGER NOT NULL,
                    updated_at_secs INTEGER NOT NULL
                 );

                 CREATE INDEX IF NOT EXISTS idx_test_plans_updated
                    ON test_plans(updated_at_secs, plan_name);

                 PRAGMA user_version = 4;",
            )?;
            tx.commit()?;
        }

        if current < 5 {
            let tx = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS intelligence_job_context (
                    job_id TEXT PRIMARY KEY NOT NULL,
                    profile_json TEXT NOT NULL,
                    changed_files_json TEXT NOT NULL,
                    created_at_secs INTEGER NOT NULL,
                    FOREIGN KEY(job_id) REFERENCES jobs(job_id) ON DELETE CASCADE
                 );
                 CREATE INDEX IF NOT EXISTS idx_intelligence_context_created
                    ON intelligence_job_context(created_at_secs, job_id);
                 PRAGMA user_version = 5;",
            )?;
            tx.commit()?;
        }

        Ok(())
    }

    pub fn upsert_test_plan(
        &mut self,
        plan: &TestPlan,
        now_secs: u64,
    ) -> Result<(), DurableControllerError> {
        plan.validate()?;
        let plan_json = serde_json::to_string(plan)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existed = tx
            .query_row(
                "SELECT 1 FROM test_plans WHERE plan_name = ?1",
                [&plan.name],
                |_row| Ok(()),
            )
            .optional()?
            .is_some();
        tx.execute(
            "INSERT INTO test_plans(
                plan_name, plan_version, plan_json, created_at_secs, updated_at_secs
             ) VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(plan_name) DO UPDATE SET
                plan_version = excluded.plan_version,
                plan_json = excluded.plan_json,
                updated_at_secs = excluded.updated_at_secs",
            params![
                plan.name,
                i64::from(plan.version),
                plan_json,
                to_i64(now_secs)?
            ],
        )?;
        insert_audit(
            &tx,
            now_secs,
            if existed {
                "test_plan_updated"
            } else {
                "test_plan_created"
            },
            "test_plan",
            &plan.name,
            &serde_json::json!({
                "version": plan.version,
                "step_count": plan.steps.len()
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_test_plan(&self, name: &str) -> Result<Option<TestPlan>, DurableControllerError> {
        let plan_json: Option<String> = self
            .connection
            .query_row(
                "SELECT plan_json FROM test_plans WHERE plan_name = ?1",
                [name],
                |row| row.get(0),
            )
            .optional()?;
        plan_json
            .map(|value| serde_json::from_str(&value).map_err(DurableControllerError::from))
            .transpose()
    }

    pub fn list_test_plans(&self) -> Result<Vec<String>, DurableControllerError> {
        let mut statement = self
            .connection
            .prepare("SELECT plan_name FROM test_plans ORDER BY plan_name")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut names = Vec::new();
        for row in rows {
            names.push(row?);
        }
        Ok(names)
    }

    pub fn enqueue_job(
        &mut self,
        job: &JobRequest,
        now_secs: u64,
    ) -> Result<(), DurableControllerError> {
        self.enqueue_job_with_retry(job, RetryPolicy::no_retry(), now_secs)
    }

    pub fn enqueue_job_with_retry(
        &mut self,
        job: &JobRequest,
        retry_policy: RetryPolicy,
        now_secs: u64,
    ) -> Result<(), DurableControllerError> {
        retry_policy.validate()?;
        let job_json = serde_json::to_string(job)?;
        let retry_policy_json = serde_json::to_string(&retry_policy)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO jobs (
                job_id, request_json, state, created_at_secs, updated_at_secs, retry_policy_json
             ) VALUES (?1, ?2, ?3, ?4, ?4, ?5)",
            params![
                job.id.to_string(),
                job_json,
                DurableJobState::Queued.as_str(),
                to_i64(now_secs)?,
                retry_policy_json
            ],
        )?;
        insert_audit(
            &tx,
            now_secs,
            "job_enqueued",
            "job",
            &job.id.to_string(),
            &serde_json::json!({
                "state":"queued",
                "retry_policy": retry_policy
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn register_worker(
        &mut self,
        worker: &WorkerRegistration,
        now_secs: u64,
    ) -> Result<(), DurableControllerError> {
        if worker.protocol_version != PROTOCOL_VERSION {
            return Err(DurableControllerError::ProtocolMismatch);
        }
        let worker_json = serde_json::to_string(worker)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO workers(worker_id, registration_json, last_seen_secs, online)
             VALUES (?1, ?2, ?3, 1)
             ON CONFLICT(worker_id) DO UPDATE SET
                registration_json = excluded.registration_json,
                last_seen_secs = excluded.last_seen_secs,
                online = 1",
            params![worker.worker_id, worker_json, to_i64(now_secs)?],
        )?;
        insert_audit(
            &tx,
            now_secs,
            "worker_registered",
            "worker",
            &worker.worker_id,
            &serde_json::json!({
                "os": worker.os,
                "arch": worker.arch,
                "protocol_version": worker.protocol_version
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn heartbeat_worker(
        &mut self,
        worker_id: &str,
        now_secs: u64,
    ) -> Result<(), DurableControllerError> {
        let changed = self.connection.execute(
            "UPDATE workers SET last_seen_secs = ?2, online = 1 WHERE worker_id = ?1",
            params![worker_id, to_i64(now_secs)?],
        )?;
        if changed == 0 {
            return Err(DurableControllerError::UnknownWorker);
        }
        Ok(())
    }

    pub fn set_worker_offline(
        &mut self,
        worker_id: &str,
        now_secs: u64,
    ) -> Result<(), DurableControllerError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE workers SET online = 0, last_seen_secs = ?2 WHERE worker_id = ?1",
            params![worker_id, to_i64(now_secs)?],
        )?;
        if changed == 0 {
            return Err(DurableControllerError::UnknownWorker);
        }
        insert_audit(
            &tx,
            now_secs,
            "worker_offline",
            "worker",
            worker_id,
            &serde_json::json!({}),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn assign_next(
        &mut self,
        worker_id: &str,
        now_secs: u64,
    ) -> Result<Option<JobRequest>, DurableControllerError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let worker_json: String = tx
            .query_row(
                "SELECT registration_json FROM workers
                 WHERE worker_id = ?1 AND online = 1",
                [worker_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(DurableControllerError::UnknownWorker)?;
        let worker: WorkerRegistration = serde_json::from_str(&worker_json)?;

        let eligible = {
            let mut statement = tx.prepare(
                "SELECT request_json, state
                 FROM jobs
                 WHERE state = 'queued'
                    OR (state = 'retry_pending' AND next_retry_at_secs IS NOT NULL
                        AND next_retry_at_secs <= ?1)
                 ORDER BY created_at_secs ASC, job_id ASC",
            )?;
            let rows = statement.query_map([to_i64(now_secs)?], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut values = Vec::new();
            for row in rows {
                values.push(row?);
            }
            values
        };

        let mut selected = None;
        for (job_json, state) in eligible {
            let job: JobRequest = serde_json::from_str(&job_json)?;
            if job.required_capabilities().is_subset(&worker.capabilities) {
                selected = Some((job, DurableJobState::parse(&state)?));
                break;
            }
        }

        let Some((job, previous_state)) = selected else {
            tx.commit()?;
            return Ok(None);
        };

        let attempt_number: i64 = tx.query_row(
            "SELECT COALESCE(MAX(attempt_number), 0) + 1
             FROM job_attempts WHERE job_id = ?1",
            [job.id.to_string()],
            |row| row.get(0),
        )?;

        let changed = tx.execute(
            "UPDATE jobs
             SET state = 'assigned', assigned_worker = ?2, updated_at_secs = ?3,
                 next_retry_at_secs = NULL,
                 failure_class = NULL,
                 retry_reason = NULL,
                 last_error = NULL
             WHERE job_id = ?1
               AND (state = 'queued'
                    OR (state = 'retry_pending' AND next_retry_at_secs IS NOT NULL
                        AND next_retry_at_secs <= ?3))",
            params![job.id.to_string(), worker_id, to_i64(now_secs)?],
        )?;
        if changed == 0 {
            return Err(DurableControllerError::InvalidTransition);
        }

        tx.execute(
            "INSERT INTO job_attempts(
                job_id, attempt_number, worker_id, state, started_at_secs
             ) VALUES (?1, ?2, ?3, 'assigned', ?4)",
            params![
                job.id.to_string(),
                attempt_number,
                worker_id,
                to_i64(now_secs)?
            ],
        )?;

        let kind = if previous_state == DurableJobState::RetryPending {
            "job_retry_assigned"
        } else {
            "job_assigned"
        };
        insert_audit(
            &tx,
            now_secs,
            kind,
            "job",
            &job.id.to_string(),
            &serde_json::json!({
                "worker_id": worker_id,
                "attempt_number": attempt_number,
                "previous_state": previous_state.as_str()
            }),
        )?;
        tx.commit()?;
        Ok(Some(job))
    }

    pub fn mark_running(
        &mut self,
        job_id: Uuid,
        now_secs: u64,
    ) -> Result<(), DurableControllerError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE jobs SET state = 'running', updated_at_secs = ?2
             WHERE job_id = ?1 AND state = 'assigned'",
            params![job_id.to_string(), to_i64(now_secs)?],
        )?;
        if changed == 0 {
            return Err(DurableControllerError::InvalidTransition);
        }
        tx.execute(
            "UPDATE job_attempts SET state = 'running'
             WHERE attempt_id = (
                SELECT attempt_id FROM job_attempts
                WHERE job_id = ?1 ORDER BY attempt_number DESC LIMIT 1
             )",
            [job_id.to_string()],
        )?;
        insert_audit(
            &tx,
            now_secs,
            "job_running",
            "job",
            &job_id.to_string(),
            &serde_json::json!({}),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn complete_job(
        &mut self,
        result: &JobResult,
        now_secs: u64,
    ) -> Result<(), DurableControllerError> {
        let failure_class = match result.status {
            df_test_protocol::JobStatus::Failed => Some(FailureClass::TestFailure),
            df_test_protocol::JobStatus::Rejected => Some(FailureClass::PolicyRejected),
            df_test_protocol::JobStatus::Cancelled => Some(FailureClass::Cancelled),
            _ => None,
        };
        self.complete_job_with_classification(result, failure_class, now_secs)
            .map(|_| ())
    }

    pub fn complete_job_with_classification(
        &mut self,
        result: &JobResult,
        failure_class: Option<FailureClass>,
        now_secs: u64,
    ) -> Result<LifecycleDecision, DurableControllerError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let (state_text, retry_policy_json): (String, Option<String>) = tx
            .query_row(
                "SELECT state, retry_policy_json FROM jobs WHERE job_id = ?1",
                [result.job_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or(DurableControllerError::UnknownJob)?;
        let current_state = DurableJobState::parse(&state_text)?;
        if !matches!(
            current_state,
            DurableJobState::Assigned | DurableJobState::Running | DurableJobState::Interrupted
        ) {
            return Err(DurableControllerError::InvalidTransition);
        }

        let retry_policy = retry_policy_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?
            .unwrap_or_else(RetryPolicy::no_retry);
        retry_policy.validate()?;

        let attempt_number_i64: i64 = tx.query_row(
            "SELECT COALESCE(MAX(attempt_number), 0)
             FROM job_attempts WHERE job_id = ?1",
            [result.job_id.to_string()],
            |row| row.get(0),
        )?;
        let attempt_number = u32::try_from(attempt_number_i64)
            .map_err(|_| DurableControllerError::IntegerOutOfRange)?;
        if attempt_number == 0 {
            return Err(DurableControllerError::MissingAttempt);
        }

        let (decision, final_state, effective_class, next_retry_at) = match result.status {
            df_test_protocol::JobStatus::Passed => (
                LifecycleDecision::TerminalPassed,
                DurableJobState::Passed,
                None,
                None,
            ),
            df_test_protocol::JobStatus::Rejected => (
                LifecycleDecision::TerminalRejected,
                DurableJobState::Rejected,
                Some(failure_class.unwrap_or(FailureClass::PolicyRejected)),
                None,
            ),
            df_test_protocol::JobStatus::Cancelled => (
                LifecycleDecision::TerminalCancelled,
                DurableJobState::Cancelled,
                Some(failure_class.unwrap_or(FailureClass::Cancelled)),
                None,
            ),
            df_test_protocol::JobStatus::Failed => {
                let class = failure_class.unwrap_or(FailureClass::TestFailure);
                let decision = decide_failure(retry_policy, class, attempt_number, now_secs)?;
                match decision {
                    LifecycleDecision::RetryScheduled { next_retry_at_secs } => (
                        decision,
                        DurableJobState::RetryPending,
                        Some(class),
                        Some(next_retry_at_secs),
                    ),
                    LifecycleDecision::RetryExhausted => {
                        (decision, DurableJobState::Exhausted, Some(class), None)
                    }
                    LifecycleDecision::TerminalRejected => {
                        (decision, DurableJobState::Rejected, Some(class), None)
                    }
                    LifecycleDecision::TerminalCancelled => {
                        (decision, DurableJobState::Cancelled, Some(class), None)
                    }
                    LifecycleDecision::InterruptedAwaitingDecision => {
                        (decision, DurableJobState::Interrupted, Some(class), None)
                    }
                    _ => (decision, DurableJobState::Failed, Some(class), None),
                }
            }
            _ => return Err(DurableControllerError::InvalidCompletionStatus),
        };

        tx.execute(
            "UPDATE jobs
             SET state = ?2,
                 assigned_worker = CASE WHEN ?2 = 'retry_pending' THEN NULL ELSE assigned_worker END,
                 updated_at_secs = ?3,
                 last_error = ?4,
                 failure_class = ?5,
                 next_retry_at_secs = ?6,
                 retry_reason = ?7
             WHERE job_id = ?1",
            params![
                result.job_id.to_string(),
                final_state.as_str(),
                to_i64(now_secs)?,
                if matches!(
                    final_state,
                    DurableJobState::Failed
                        | DurableJobState::Exhausted
                        | DurableJobState::Interrupted
                        | DurableJobState::RetryPending
                ) {
                    Some(result.summary.as_str())
                } else {
                    None
                },
                effective_class.map(failure_class_as_str),
                next_retry_at.map(to_i64).transpose()?,
                if final_state == DurableJobState::RetryPending {
                    Some(result.summary.as_str())
                } else {
                    None
                }
            ],
        )?;

        tx.execute(
            "UPDATE job_attempts
             SET state = ?2, finished_at_secs = ?3, summary = ?4, failure_class = ?5
             WHERE attempt_id = (
                SELECT attempt_id FROM job_attempts
                WHERE job_id = ?1 ORDER BY attempt_number DESC LIMIT 1
             )",
            params![
                result.job_id.to_string(),
                attempt_terminal_state(result.status),
                to_i64(now_secs)?,
                result.summary,
                effective_class.map(failure_class_as_str)
            ],
        )?;

        tx.execute(
            "DELETE FROM artifact_metadata WHERE job_id = ?1",
            [result.job_id.to_string()],
        )?;
        for artifact in &result.artifacts {
            insert_artifact(&tx, result.job_id, artifact, now_secs)?;
        }

        let audit_kind = match decision {
            LifecycleDecision::RetryScheduled { .. } => "job_retry_scheduled",
            LifecycleDecision::RetryExhausted => "job_retry_exhausted",
            LifecycleDecision::InterruptedAwaitingDecision => "job_interrupted_waiting",
            _ => "job_completed",
        };
        insert_audit(
            &tx,
            now_secs,
            audit_kind,
            "job",
            &result.job_id.to_string(),
            &serde_json::json!({
                "state": final_state.as_str(),
                "summary": result.summary,
                "artifact_count": result.artifacts.len(),
                "failure_class": effective_class,
                "attempt_number": attempt_number,
                "next_retry_at_secs": next_retry_at
            }),
        )?;
        tx.commit()?;
        Ok(decision)
    }

    pub fn cancel_queued_job(
        &mut self,
        job_id: Uuid,
        now_secs: u64,
    ) -> Result<bool, DurableControllerError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous_state: Option<String> = tx
            .query_row(
                "SELECT state FROM jobs
                 WHERE job_id = ?1 AND state IN ('queued', 'retry_pending')",
                [job_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        let changed = tx.execute(
            "UPDATE jobs
             SET state = 'cancelled',
                 updated_at_secs = ?2,
                 failure_class = 'cancelled',
                 next_retry_at_secs = NULL,
                 retry_reason = NULL
             WHERE job_id = ?1 AND state IN ('queued', 'retry_pending')",
            params![job_id.to_string(), to_i64(now_secs)?],
        )?;
        if changed > 0 {
            insert_audit(
                &tx,
                now_secs,
                "job_cancelled",
                "job",
                &job_id.to_string(),
                &serde_json::json!({
                    "from": previous_state.as_deref().unwrap_or("unknown")
                }),
            )?;
        }
        tx.commit()?;
        Ok(changed > 0)
    }

    pub fn recover_after_restart(
        &mut self,
        now_secs: u64,
    ) -> Result<Vec<Uuid>, DurableControllerError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let interrupted = {
            let mut statement = tx.prepare(
                "SELECT job_id, retry_policy_json
                 FROM jobs
                 WHERE state IN ('assigned', 'running')
                 ORDER BY created_at_secs, job_id",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?;
            let mut values = Vec::new();
            for row in rows {
                values.push(row?);
            }
            values
        };

        let mut interrupted_ids = Vec::new();
        for (job_id_text, retry_policy_json) in interrupted {
            let job_id = Uuid::parse_str(&job_id_text)?;
            interrupted_ids.push(job_id);
            let attempt_number_i64: i64 = tx.query_row(
                "SELECT COALESCE(MAX(attempt_number), 0)
                 FROM job_attempts WHERE job_id = ?1",
                [job_id_text.as_str()],
                |row| row.get(0),
            )?;
            let attempt_number = u32::try_from(attempt_number_i64)
                .map_err(|_| DurableControllerError::IntegerOutOfRange)?;
            if attempt_number == 0 {
                return Err(DurableControllerError::MissingAttempt);
            }

            let policy = retry_policy_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?
                .unwrap_or_else(RetryPolicy::no_retry);
            let decision =
                decide_failure(policy, FailureClass::Interrupted, attempt_number, now_secs)?;

            let (state, next_retry_at) = match decision {
                LifecycleDecision::RetryScheduled { next_retry_at_secs } => {
                    (DurableJobState::RetryPending, Some(next_retry_at_secs))
                }
                LifecycleDecision::RetryExhausted => (DurableJobState::Exhausted, None),
                _ => (DurableJobState::Interrupted, None),
            };

            tx.execute(
                "UPDATE jobs
                 SET state = ?2,
                     assigned_worker = NULL,
                     updated_at_secs = ?3,
                     last_error = 'controller restarted while job was in flight',
                     failure_class = 'interrupted',
                     next_retry_at_secs = ?4,
                     retry_reason = CASE WHEN ?2 = 'retry_pending'
                         THEN 'controller restarted while job was in flight' ELSE NULL END
                 WHERE job_id = ?1",
                params![
                    job_id_text,
                    state.as_str(),
                    to_i64(now_secs)?,
                    next_retry_at.map(to_i64).transpose()?
                ],
            )?;
            tx.execute(
                "UPDATE job_attempts
                 SET state = 'interrupted',
                     finished_at_secs = ?2,
                     summary = 'controller restarted while job was in flight',
                     failure_class = 'interrupted'
                 WHERE attempt_id = (
                    SELECT attempt_id FROM job_attempts
                    WHERE job_id = ?1 ORDER BY attempt_number DESC LIMIT 1
                 )",
                params![job_id.to_string(), to_i64(now_secs)?],
            )?;

            let kind = if state == DurableJobState::RetryPending {
                "job_interrupted_retry_scheduled"
            } else if state == DurableJobState::Exhausted {
                "job_interrupted_retry_exhausted"
            } else {
                "job_interrupted_on_recovery"
            };
            insert_audit(
                &tx,
                now_secs,
                kind,
                "job",
                &job_id.to_string(),
                &serde_json::json!({
                    "attempt_number": attempt_number,
                    "state": state.as_str(),
                    "next_retry_at_secs": next_retry_at
                }),
            )?;
        }

        tx.commit()?;
        Ok(interrupted_ids)
    }

    pub fn reschedule_interrupted_job(
        &mut self,
        job_id: Uuid,
        now_secs: u64,
    ) -> Result<(), DurableControllerError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (state, attempt_count): (String, i64) = tx
            .query_row(
                "SELECT state,
                        (SELECT COUNT(*) FROM job_attempts WHERE job_id = jobs.job_id)
                 FROM jobs WHERE job_id = ?1",
                [job_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or(DurableControllerError::UnknownJob)?;
        if state != DurableJobState::Interrupted.as_str() {
            return Err(DurableControllerError::InvalidTransition);
        }
        let attempt_count =
            u32::try_from(attempt_count).map_err(|_| DurableControllerError::IntegerOutOfRange)?;
        if attempt_count >= df_test_lifecycle::MAX_ATTEMPTS {
            return Err(DurableControllerError::ManualRescheduleLimitReached);
        }

        let changed = tx.execute(
            "UPDATE jobs
             SET state = 'queued',
                 assigned_worker = NULL,
                 updated_at_secs = ?2,
                 failure_class = NULL,
                 next_retry_at_secs = NULL,
                 retry_reason = NULL,
                 last_error = NULL
             WHERE job_id = ?1 AND state = 'interrupted'",
            params![job_id.to_string(), to_i64(now_secs)?],
        )?;
        if changed == 0 {
            return Err(DurableControllerError::InvalidTransition);
        }
        insert_audit(
            &tx,
            now_secs,
            "job_interrupted_manually_rescheduled",
            "job",
            &job_id.to_string(),
            &serde_json::json!({"state":"queued"}),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_job(
        &self,
        job_id: Uuid,
    ) -> Result<Option<DurableJobRecord>, DurableControllerError> {
        let row = self
            .connection
            .query_row(
                "SELECT request_json, state, assigned_worker,
                        created_at_secs, updated_at_secs, last_error,
                        retry_policy_json, failure_class, next_retry_at_secs, retry_reason
                 FROM jobs WHERE job_id = ?1",
                [job_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, Option<i64>>(8)?,
                        row.get::<_, Option<String>>(9)?,
                    ))
                },
            )
            .optional()?;

        row.map(
            |(
                job_json,
                state,
                assigned_worker,
                created,
                updated,
                last_error,
                retry_policy_json,
                failure_class,
                next_retry_at,
                retry_reason,
            )| {
                Ok(DurableJobRecord {
                    job: serde_json::from_str(&job_json)?,
                    state: DurableJobState::parse(&state)?,
                    assigned_worker,
                    created_at_secs: to_u64(created)?,
                    updated_at_secs: to_u64(updated)?,
                    last_error,
                    retry_policy: retry_policy_json
                        .as_deref()
                        .map(serde_json::from_str)
                        .transpose()?
                        .unwrap_or_else(RetryPolicy::no_retry),
                    failure_class: failure_class
                        .as_deref()
                        .map(parse_failure_class)
                        .transpose()?,
                    next_retry_at_secs: next_retry_at.map(to_u64).transpose()?,
                    retry_reason,
                })
            },
        )
        .transpose()
    }

    pub fn list_attempts(
        &self,
        job_id: Uuid,
    ) -> Result<Vec<DurableAttemptRecord>, DurableControllerError> {
        let mut statement = self.connection.prepare(
            "SELECT attempt_id, attempt_number, worker_id, state,
                    started_at_secs, finished_at_secs, summary, failure_class
             FROM job_attempts
             WHERE job_id = ?1
             ORDER BY attempt_number",
        )?;
        let rows = statement.query_map([job_id.to_string()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })?;

        let mut attempts = Vec::new();
        for row in rows {
            let (
                attempt_id,
                attempt_number,
                worker_id,
                state,
                started,
                finished,
                summary,
                failure_class,
            ) = row?;
            attempts.push(DurableAttemptRecord {
                attempt_id,
                job_id,
                attempt_number: u32::try_from(attempt_number)
                    .map_err(|_| DurableControllerError::IntegerOutOfRange)?,
                worker_id,
                state: DurableJobState::parse(&state)?,
                started_at_secs: started.map(to_u64).transpose()?,
                finished_at_secs: finished.map(to_u64).transpose()?,
                summary,
                failure_class: failure_class
                    .as_deref()
                    .map(parse_failure_class)
                    .transpose()?,
            });
        }
        Ok(attempts)
    }

    pub fn list_workers(&self) -> Result<Vec<DurableWorkerRecord>, DurableControllerError> {
        let mut statement = self.connection.prepare(
            "SELECT registration_json, last_seen_secs, online
             FROM workers ORDER BY worker_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;

        let mut workers = Vec::new();
        for row in rows {
            let (registration_json, last_seen, online) = row?;
            workers.push(DurableWorkerRecord {
                registration: serde_json::from_str(&registration_json)?,
                last_seen_secs: to_u64(last_seen)?,
                online: online != 0,
            });
        }
        Ok(workers)
    }

    pub fn list_artifacts(&self, job_id: Uuid) -> Result<Vec<ArtifactRef>, DurableControllerError> {
        let mut statement = self.connection.prepare(
            "SELECT name, relative_path, size_bytes, sha256
             FROM artifact_metadata
             WHERE job_id = ?1
             ORDER BY artifact_id",
        )?;
        let rows = statement.query_map([job_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;
        let mut artifacts = Vec::new();
        for row in rows {
            let (name, relative_path, size_bytes, sha256) = row?;
            artifacts.push(ArtifactRef {
                name,
                relative_path,
                size_bytes: to_u64(size_bytes)?,
                sha256,
            });
        }
        Ok(artifacts)
    }

    pub fn record_intelligence_job_context(
        &mut self,
        job_id: Uuid,
        profile: TestProfile,
        changed_files: &[String],
        now_secs: u64,
    ) -> Result<(), DurableControllerError> {
        let changes = ChangeSet {
            files: changed_files.to_vec(),
        };
        changes.validate()?;
        if self.get_job(job_id)?.is_none() {
            return Err(DurableControllerError::UnknownJob);
        }
        let profile_json = serde_json::to_string(&profile)?;
        let files_json = serde_json::to_string(changed_files)?;
        self.connection.execute(
            "INSERT INTO intelligence_job_context(job_id, profile_json, changed_files_json, created_at_secs)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(job_id) DO UPDATE SET
                profile_json = excluded.profile_json,
                changed_files_json = excluded.changed_files_json,
                created_at_secs = excluded.created_at_secs",
            params![
                job_id.to_string(),
                profile_json,
                files_json,
                to_i64(now_secs)?
            ],
        )?;
        Ok(())
    }

    pub fn intelligence_historical_failures(
        &self,
        limit: usize,
    ) -> Result<Vec<HistoricalFailure>, DurableControllerError> {
        if limit == 0 || limit > MAX_HISTORY_RECORDS {
            return Err(DurableControllerError::InvalidQueryLimit);
        }
        let mut statement = self.connection.prepare(
            "SELECT j.job_id, c.profile_json, j.last_error, c.changed_files_json, j.updated_at_secs
             FROM jobs j
             INNER JOIN intelligence_job_context c ON c.job_id = j.job_id
             WHERE j.state IN ('failed','exhausted')
               AND j.failure_class = 'test_failure'
             ORDER BY j.updated_at_secs DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map(
            [i64::try_from(limit).map_err(|_| DurableControllerError::IntegerOutOfRange)?],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )?;
        let mut failures = Vec::new();
        for row in rows {
            let (job_id, profile_json, message, changed_files_json, updated_at) = row?;
            failures.push(HistoricalFailure {
                id: Uuid::parse_str(&job_id)?,
                profile: serde_json::from_str(&profile_json)?,
                step: "plan_job".into(),
                message: message.unwrap_or_else(|| "test job failed".into()),
                changed_files: serde_json::from_str(&changed_files_json)?,
                unix_time_secs: to_u64(updated_at)?,
            });
        }
        Ok(failures)
    }

    pub fn intelligence_worker_capacities(
        &self,
    ) -> Result<Vec<WorkerCapacity>, DurableControllerError> {
        let workers = self.list_workers()?;
        let mut result = Vec::new();
        for worker in workers.into_iter().filter(|worker| worker.online) {
            let active_i64: i64 = self.connection.query_row(
                "SELECT COUNT(*) FROM jobs
                 WHERE assigned_worker = ?1 AND state IN ('assigned','running')",
                [&worker.registration.worker_id],
                |row| row.get(0),
            )?;
            let active_jobs = u16::try_from(active_i64)
                .map_err(|_| DurableControllerError::IntegerOutOfRange)?;
            let capabilities = &worker.registration.capabilities;
            let mut supported_profiles = std::collections::BTreeSet::new();
            let fast_required = [
                df_test_protocol::Capability::CheckoutRepository,
                df_test_protocol::Capability::CargoFmtCheck,
                df_test_protocol::Capability::CargoTest,
            ];
            if fast_required.iter().all(|cap| capabilities.contains(cap)) {
                supported_profiles.insert(TestProfile::RustFast);
            }
            let standard_required = [
                df_test_protocol::Capability::CheckoutRepository,
                df_test_protocol::Capability::CargoFmtCheck,
                df_test_protocol::Capability::CargoClippy,
                df_test_protocol::Capability::CargoTest,
            ];
            if standard_required.iter().all(|cap| capabilities.contains(cap)) {
                supported_profiles.insert(TestProfile::RustStandard);
            }
            if supported_profiles.is_empty() {
                continue;
            }
            let max_parallel_jobs = 1u16;
            result.push(WorkerCapacity {
                worker_id: worker.registration.worker_id,
                supported_profiles,
                total_memory_mib: 8192,
                free_memory_mib: if active_jobs == 0 { 8192 } else { 0 },
                max_parallel_jobs,
                active_jobs: active_jobs.min(max_parallel_jobs),
                load_percent: if active_jobs == 0 { 0 } else { 100 },
            });
        }
        Ok(result)
    }

    pub fn record_intelligence_decision(
        &mut self,
        decision_id: Uuid,
        mode: &str,
        report: &serde_json::Value,
        now_secs: u64,
    ) -> Result<i64, DurableControllerError> {
        let report_json = serde_json::to_string(report)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO intelligence_history(report_json, created_at_secs)
             VALUES (?1, ?2)",
            params![report_json, to_i64(now_secs)?],
        )?;
        let row_id = tx.last_insert_rowid();
        insert_audit(
            &tx,
            now_secs,
            "intelligence_decision",
            "intelligence_decision",
            &decision_id.to_string(),
            &serde_json::json!({"mode": mode, "history_id": row_id}),
        )?;
        tx.commit()?;
        Ok(row_id)
    }

    pub fn record_intelligence(
        &mut self,
        report: &serde_json::Value,
        now_secs: u64,
    ) -> Result<i64, DurableControllerError> {
        let report_json = serde_json::to_string(report)?;
        self.connection.execute(
            "INSERT INTO intelligence_history(report_json, created_at_secs)
             VALUES (?1, ?2)",
            params![report_json, to_i64(now_secs)?],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn set_config(
        &mut self,
        key: &str,
        value: &str,
        now_secs: u64,
    ) -> Result<(), DurableControllerError> {
        validate_config_key(key)?;
        self.connection.execute(
            "INSERT INTO controller_config(config_key, config_value, updated_at_secs)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(config_key) DO UPDATE SET
                config_value = excluded.config_value,
                updated_at_secs = excluded.updated_at_secs",
            params![key, value, to_i64(now_secs)?],
        )?;
        Ok(())
    }

    pub fn get_config(&self, key: &str) -> Result<Option<String>, DurableControllerError> {
        validate_config_key(key)?;
        Ok(self
            .connection
            .query_row(
                "SELECT config_value FROM controller_config WHERE config_key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn audit_events_for(
        &self,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<Vec<AuditEvent>, DurableControllerError> {
        let mut statement = self.connection.prepare(
            "SELECT audit_id, unix_time_secs, kind, entity_type, entity_id, detail_json,
                    previous_sha256, event_sha256
             FROM audit_events
             WHERE entity_type = ?1 AND entity_id = ?2
             ORDER BY audit_id",
        )?;
        let rows = statement.query_map(params![entity_type, entity_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })?;
        let mut events = Vec::new();
        for row in rows {
            let (
                id,
                unix_time,
                kind,
                stored_entity_type,
                stored_entity_id,
                detail_json,
                previous_sha256,
                event_sha256,
            ) = row?;
            events.push(AuditEvent {
                id,
                unix_time_secs: to_u64(unix_time)?,
                kind,
                entity_type: stored_entity_type,
                entity_id: stored_entity_id,
                detail: serde_json::from_str(&detail_json)?,
                previous_sha256,
                event_sha256,
            });
        }
        Ok(events)
    }

    pub fn verify_audit_chain(&self) -> Result<bool, DurableControllerError> {
        let mut statement = self.connection.prepare(
            "SELECT audit_id, unix_time_secs, kind, entity_type, entity_id, detail_json,
                    previous_sha256, event_sha256
             FROM audit_events
             WHERE event_sha256 IS NOT NULL
             ORDER BY audit_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })?;

        let mut expected_previous: Option<String> = None;
        for row in rows {
            let (
                id,
                unix_time,
                kind,
                entity_type,
                entity_id,
                detail_json,
                stored_previous,
                stored_hash,
            ) = row?;
            let detail: serde_json::Value = serde_json::from_str(&detail_json)?;
            let payload = serde_json::json!({
                "unix_time_secs": to_u64(unix_time)?,
                "kind": kind,
                "entity_type": entity_type,
                "entity_id": entity_id,
                "detail": detail
            });
            let digest = next_audit_digest(to_u64(id)?, expected_previous.as_deref(), &payload)?;
            if stored_previous.as_deref() != Some(digest.previous_sha256.as_str())
                || stored_hash.as_deref() != Some(digest.event_sha256.as_str())
            {
                return Ok(false);
            }
            expected_previous = Some(digest.event_sha256);
        }
        Ok(true)
    }

    pub fn record_log(
        &mut self,
        event: &StructuredLogEvent,
    ) -> Result<i64, DurableControllerError> {
        event.validate()?;
        let event = event.clone().redacted();
        self.connection.execute(
            "INSERT INTO structured_logs(
                unix_time_secs, level, component, message, fields_json, job_id, worker_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                to_i64(event.unix_time_secs)?,
                log_level_as_str(event.level),
                event.component,
                event.message,
                serde_json::to_string(&event.fields)?,
                event.job_id,
                event.worker_id
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn recent_logs(
        &self,
        limit: usize,
    ) -> Result<Vec<DurableLogRecord>, DurableControllerError> {
        let limit = bounded_query_limit(limit)?;
        let mut statement = self.connection.prepare(
            "SELECT log_id, unix_time_secs, level, component, message, fields_json, job_id, worker_id
             FROM structured_logs
             ORDER BY log_id DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map(
            [i64::try_from(limit).map_err(|_| DurableControllerError::IntegerOutOfRange)?],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            },
        )?;
        let mut records = Vec::new();
        for row in rows {
            let (id, unix_time, level, component, message, fields_json, job_id, worker_id) = row?;
            records.push(DurableLogRecord {
                id,
                event: StructuredLogEvent {
                    unix_time_secs: to_u64(unix_time)?,
                    level: parse_log_level(&level)?,
                    component,
                    message,
                    fields: serde_json::from_str(&fields_json)?,
                    job_id,
                    worker_id,
                },
            });
        }
        Ok(records)
    }

    pub fn record_metric(&mut self, point: &MetricPoint) -> Result<i64, DurableControllerError> {
        point.validate()?;
        self.connection.execute(
            "INSERT INTO metric_samples(unix_time_secs, name, value, labels_json)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                to_i64(point.unix_time_secs)?,
                point.name,
                point.value,
                serde_json::to_string(&point.labels)?
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn recent_metrics(
        &self,
        limit: usize,
    ) -> Result<Vec<DurableMetricRecord>, DurableControllerError> {
        let limit = bounded_query_limit(limit)?;
        let mut statement = self.connection.prepare(
            "SELECT metric_id, unix_time_secs, name, value, labels_json
             FROM metric_samples
             ORDER BY metric_id DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map(
            [i64::try_from(limit).map_err(|_| DurableControllerError::IntegerOutOfRange)?],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )?;
        let mut records = Vec::new();
        for row in rows {
            let (id, unix_time, name, value, labels_json) = row?;
            records.push(DurableMetricRecord {
                id,
                point: MetricPoint {
                    unix_time_secs: to_u64(unix_time)?,
                    name,
                    value,
                    labels: serde_json::from_str(&labels_json)?,
                },
            });
        }
        Ok(records)
    }

    pub fn list_artifact_records(
        &self,
        job_id: Uuid,
    ) -> Result<Vec<DurableArtifactRecord>, DurableControllerError> {
        let mut statement = self.connection.prepare(
            "SELECT artifact_id, name, relative_path, size_bytes, sha256,
                    created_at_secs, retained_until_secs
             FROM artifact_metadata
             WHERE job_id = ?1
             ORDER BY artifact_id",
        )?;
        let rows = statement.query_map([job_id.to_string()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, Option<i64>>(6)?,
            ))
        })?;
        let mut records = Vec::new();
        for row in rows {
            let (id, name, relative_path, size_bytes, sha256, created_at, retained_until) = row?;
            records.push(DurableArtifactRecord {
                id,
                job_id,
                artifact: ArtifactRef {
                    name,
                    relative_path,
                    size_bytes: to_u64(size_bytes)?,
                    sha256,
                },
                created_at_secs: to_u64(created_at)?,
                retained_until_secs: retained_until.map(to_u64).transpose()?,
            });
        }
        Ok(records)
    }

    pub fn set_artifact_retention(
        &mut self,
        artifact_id: i64,
        retained_until_secs: Option<u64>,
    ) -> Result<bool, DurableControllerError> {
        let retained_until = retained_until_secs.map(to_i64).transpose()?;
        let changed = self.connection.execute(
            "UPDATE artifact_metadata
             SET retained_until_secs = ?2
             WHERE artifact_id = ?1",
            params![artifact_id, retained_until],
        )?;
        Ok(changed > 0)
    }

    pub fn prune_telemetry_before(
        &mut self,
        before_secs: u64,
    ) -> Result<(usize, usize), DurableControllerError> {
        let before = to_i64(before_secs)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let logs = tx.execute(
            "DELETE FROM structured_logs WHERE unix_time_secs < ?1",
            [before],
        )?;
        let metrics = tx.execute(
            "DELETE FROM metric_samples WHERE unix_time_secs < ?1",
            [before],
        )?;
        tx.commit()?;
        Ok((logs, metrics))
    }
}

fn insert_artifact(
    tx: &rusqlite::Transaction<'_>,
    job_id: Uuid,
    artifact: &ArtifactRef,
    now_secs: u64,
) -> Result<(), DurableControllerError> {
    tx.execute(
        "INSERT INTO artifact_metadata(
            job_id, name, relative_path, size_bytes, sha256, created_at_secs
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            job_id.to_string(),
            artifact.name,
            artifact.relative_path,
            to_i64(artifact.size_bytes)?,
            artifact.sha256,
            to_i64(now_secs)?
        ],
    )?;
    Ok(())
}

fn insert_audit(
    tx: &rusqlite::Transaction<'_>,
    now_secs: u64,
    kind: &str,
    entity_type: &str,
    entity_id: &str,
    detail: &serde_json::Value,
) -> Result<(), DurableControllerError> {
    let sequence: i64 = tx.query_row(
        "SELECT COALESCE(MAX(audit_id), 0) + 1 FROM audit_events",
        [],
        |row| row.get(0),
    )?;
    let previous: Option<String> = tx
        .query_row(
            "SELECT event_sha256 FROM audit_events
             WHERE event_sha256 IS NOT NULL
             ORDER BY audit_id DESC
             LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let payload = serde_json::json!({
        "unix_time_secs": now_secs,
        "kind": kind,
        "entity_type": entity_type,
        "entity_id": entity_id,
        "detail": detail
    });
    let digest = next_audit_digest(to_u64(sequence)?, previous.as_deref(), &payload)?;
    tx.execute(
        "INSERT INTO audit_events(
            unix_time_secs, kind, entity_type, entity_id, detail_json,
            previous_sha256, event_sha256
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            to_i64(now_secs)?,
            kind,
            entity_type,
            entity_id,
            serde_json::to_string(detail)?,
            digest.previous_sha256,
            digest.event_sha256
        ],
    )?;
    Ok(())
}

fn failure_class_as_str(class: FailureClass) -> &'static str {
    match class {
        FailureClass::TestFailure => "test_failure",
        FailureClass::InfrastructureTransient => "infrastructure_transient",
        FailureClass::InfrastructurePermanent => "infrastructure_permanent",
        FailureClass::PolicyRejected => "policy_rejected",
        FailureClass::Cancelled => "cancelled",
        FailureClass::Interrupted => "interrupted",
    }
}

fn parse_failure_class(value: &str) -> Result<FailureClass, DurableControllerError> {
    match value {
        "test_failure" => Ok(FailureClass::TestFailure),
        "infrastructure_transient" => Ok(FailureClass::InfrastructureTransient),
        "infrastructure_permanent" => Ok(FailureClass::InfrastructurePermanent),
        "policy_rejected" => Ok(FailureClass::PolicyRejected),
        "cancelled" => Ok(FailureClass::Cancelled),
        "interrupted" => Ok(FailureClass::Interrupted),
        _ => Err(DurableControllerError::InvalidStoredFailureClass(
            value.to_owned(),
        )),
    }
}

fn attempt_terminal_state(status: df_test_protocol::JobStatus) -> &'static str {
    match status {
        df_test_protocol::JobStatus::Passed => "passed",
        df_test_protocol::JobStatus::Failed => "failed",
        df_test_protocol::JobStatus::Rejected => "rejected",
        df_test_protocol::JobStatus::Cancelled => "cancelled",
        _ => "failed",
    }
}

fn log_level_as_str(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Trace => "trace",
        LogLevel::Debug => "debug",
        LogLevel::Info => "info",
        LogLevel::Warn => "warn",
        LogLevel::Error => "error",
    }
}

fn parse_log_level(value: &str) -> Result<LogLevel, DurableControllerError> {
    match value {
        "trace" => Ok(LogLevel::Trace),
        "debug" => Ok(LogLevel::Debug),
        "info" => Ok(LogLevel::Info),
        "warn" => Ok(LogLevel::Warn),
        "error" => Ok(LogLevel::Error),
        _ => Err(DurableControllerError::InvalidStoredLogLevel(
            value.to_owned(),
        )),
    }
}

fn bounded_query_limit(limit: usize) -> Result<usize, DurableControllerError> {
    if limit == 0 || limit > 10_000 {
        return Err(DurableControllerError::InvalidQueryLimit);
    }
    Ok(limit)
}

fn validate_config_key(key: &str) -> Result<(), DurableControllerError> {
    if key.is_empty()
        || key.len() > 128
        || !key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'))
    {
        return Err(DurableControllerError::InvalidConfigKey);
    }
    Ok(())
}

fn to_i64(value: u64) -> Result<i64, DurableControllerError> {
    i64::try_from(value).map_err(|_| DurableControllerError::IntegerOutOfRange)
}

fn to_u64(value: i64) -> Result<u64, DurableControllerError> {
    u64::try_from(value).map_err(|_| DurableControllerError::IntegerOutOfRange)
}

#[derive(Debug, Error)]
pub enum DurableControllerError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("UUID parse error: {0}")]
    Uuid(#[from] uuid::Error),
    #[error("controller database schema {current} is newer than supported schema {supported}")]
    SchemaTooNew { current: i64, supported: i64 },
    #[error("stored controller state is invalid: {0}")]
    InvalidStoredState(String),
    #[error("worker protocol version mismatch")]
    ProtocolMismatch,
    #[error("worker is not registered or not online")]
    UnknownWorker,
    #[error("job was not found")]
    UnknownJob,
    #[error("invalid durable job state transition")]
    InvalidTransition,
    #[error("completion result does not contain a terminal status")]
    InvalidCompletionStatus,
    #[error("job has no durable attempt to classify")]
    MissingAttempt,
    #[error("manual rescheduling reached the global attempt safety limit")]
    ManualRescheduleLimitReached,
    #[error("integer value is outside the supported SQLite range")]
    IntegerOutOfRange,
    #[error("invalid controller configuration key")]
    InvalidConfigKey,
    #[error("invalid stored log level: {0}")]
    InvalidStoredLogLevel(String),
    #[error("query limit must be between 1 and 10000")]
    InvalidQueryLimit,
    #[error("observability validation error: {0}")]
    Observability(#[from] df_test_observability::ObservabilityError),
    #[error("job lifecycle validation error: {0}")]
    Lifecycle(#[from] df_test_lifecycle::LifecycleError),
    #[error("test intelligence validation error: {0}")]
    Intelligence(#[from] df_test_intelligence::IntelligenceError),
    #[error("test plan validation error: {0}")]
    Plan(#[from] df_test_plans::PlanError),
    #[error("invalid stored failure classification: {0}")]
    InvalidStoredFailureClass(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use df_test_lifecycle::{FailureClass, LifecycleDecision, RetryPolicy};
    use df_test_observability::{LogLevel, MetricPoint, StructuredLogEvent};
    use df_test_plans::{
        ArtifactKind, PlanCondition, PlanProfile, PlanStep, TargetOs, TestPlan, TEST_PLAN_VERSION,
    };
    use df_test_protocol::{Capability, JobStatus, RepositorySpec, TestAction};
    use std::{collections::BTreeSet, fs};

    fn test_job() -> JobRequest {
        JobRequest::new(
            RepositorySpec {
                url: "https://github.com/example/project.git".into(),
                revision: "main".into(),
            },
            vec![
                TestAction::Checkout,
                TestAction::CargoTest { all_features: true },
            ],
        )
    }

    fn test_worker() -> WorkerRegistration {
        WorkerRegistration {
            worker_id: "windows-1".into(),
            protocol_version: PROTOCOL_VERSION,
            os: "windows".into(),
            arch: "x86_64".into(),
            capabilities: BTreeSet::from([Capability::CheckoutRepository, Capability::CargoTest]),
        }
    }

    #[test]
    fn scheduler_skips_jobs_worker_cannot_run() {
        let mut controller = Controller::default();
        controller
            .register_worker(WorkerRegistration {
                worker_id: "linux-1".into(),
                protocol_version: PROTOCOL_VERSION,
                os: "linux".into(),
                arch: "x86_64".into(),
                capabilities: [Capability::CargoTest].into_iter().collect(),
            })
            .unwrap();

        controller.enqueue(JobRequest::new(
            RepositorySpec {
                url: "https://github.com/example/a.git".into(),
                revision: "main".into(),
            },
            vec![TestAction::CargoBuild { release: false }],
        ));
        controller.enqueue(JobRequest::new(
            RepositorySpec {
                url: "https://github.com/example/b.git".into(),
                revision: "main".into(),
            },
            vec![TestAction::CargoTest {
                all_features: false,
            }],
        ));

        let assigned = controller.assign_next("linux-1").unwrap().unwrap();
        assert!(matches!(assigned.actions[0], TestAction::CargoTest { .. }));
        assert_eq!(controller.queued_jobs(), 1);
    }

    #[test]
    fn durable_state_survives_reopen_and_completion() {
        let path =
            std::env::temp_dir().join(format!("dragonforge-phase11-{}.sqlite3", Uuid::new_v4()));
        let job = test_job();

        {
            let mut controller = DurableController::open(&path).unwrap();
            controller.enqueue_job(&job, 10).unwrap();
            controller.register_worker(&test_worker(), 11).unwrap();
            let assigned = controller.assign_next("windows-1", 12).unwrap().unwrap();
            assert_eq!(assigned.id, job.id);
            controller.mark_running(job.id, 13).unwrap();
        }

        {
            let mut controller = DurableController::open(&path).unwrap();
            let interrupted = controller.recover_after_restart(20).unwrap();
            assert_eq!(interrupted, vec![job.id]);
            let record = controller.get_job(job.id).unwrap().unwrap();
            assert_eq!(record.state, DurableJobState::Interrupted);

            controller
                .complete_job(
                    &JobResult {
                        job_id: job.id,
                        status: JobStatus::Failed,
                        summary: "interrupted test fixture".into(),
                        artifacts: vec![ArtifactRef {
                            name: "fixture.log".into(),
                            relative_path: "logs/fixture.log".into(),
                            size_bytes: 42,
                            sha256: Some("a".repeat(64)),
                        }],
                    },
                    21,
                )
                .unwrap();

            let completed = controller.get_job(job.id).unwrap().unwrap();
            assert_eq!(completed.state, DurableJobState::Failed);
            assert_eq!(controller.list_artifacts(job.id).unwrap().len(), 1);
            assert_eq!(controller.list_attempts(job.id).unwrap().len(), 1);
            assert!(controller
                .audit_events_for("job", &job.id.to_string())
                .unwrap()
                .iter()
                .any(|event| event.kind == "job_interrupted_on_recovery"));
        }

        let _ = fs::remove_file(path);
    }

    #[test]
    fn queued_jobs_remain_queued_after_restart() {
        let path = std::env::temp_dir().join(format!(
            "dragonforge-phase11-queued-{}.sqlite3",
            Uuid::new_v4()
        ));
        let job = test_job();

        {
            let mut controller = DurableController::open(&path).unwrap();
            controller.enqueue_job(&job, 100).unwrap();
        }
        {
            let mut controller = DurableController::open(&path).unwrap();
            assert!(controller.recover_after_restart(101).unwrap().is_empty());
            assert_eq!(
                controller.get_job(job.id).unwrap().unwrap().state,
                DurableJobState::Queued
            );
        }

        let _ = fs::remove_file(path);
    }

    #[test]
    fn durable_scheduler_respects_worker_capabilities() {
        let mut controller = DurableController::open_in_memory().unwrap();
        let incompatible = JobRequest::new(
            RepositorySpec {
                url: "https://github.com/example/build.git".into(),
                revision: "main".into(),
            },
            vec![TestAction::CargoBuild { release: false }],
        );
        let compatible = test_job();
        controller.enqueue_job(&incompatible, 1).unwrap();
        controller.enqueue_job(&compatible, 2).unwrap();
        controller.register_worker(&test_worker(), 3).unwrap();

        let assigned = controller.assign_next("windows-1", 4).unwrap().unwrap();
        assert_eq!(assigned.id, compatible.id);
        assert_eq!(
            controller.get_job(incompatible.id).unwrap().unwrap().state,
            DurableJobState::Queued
        );
    }

    #[test]
    fn controller_config_and_intelligence_history_are_persistent() {
        let mut controller = DurableController::open_in_memory().unwrap();
        controller
            .set_config("controller.mode", "local", 1)
            .unwrap();
        assert_eq!(
            controller.get_config("controller.mode").unwrap().as_deref(),
            Some("local")
        );
        let id = controller
            .record_intelligence(&serde_json::json!({"profile":"rust_standard"}), 2)
            .unwrap();
        assert!(id > 0);
        assert_eq!(controller.schema_version().unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn schema_v1_migrates_through_v5_intelligence_context() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE jobs (
                    job_id TEXT PRIMARY KEY NOT NULL,
                    request_json TEXT NOT NULL,
                    state TEXT NOT NULL,
                    assigned_worker TEXT,
                    created_at_secs INTEGER NOT NULL,
                    updated_at_secs INTEGER NOT NULL,
                    last_error TEXT
                 );
                 CREATE TABLE job_attempts (
                    attempt_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    job_id TEXT NOT NULL,
                    attempt_number INTEGER NOT NULL,
                    worker_id TEXT,
                    state TEXT NOT NULL,
                    started_at_secs INTEGER,
                    finished_at_secs INTEGER,
                    summary TEXT
                 );
                 CREATE TABLE artifact_metadata (
                    artifact_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    job_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    relative_path TEXT NOT NULL,
                    size_bytes INTEGER NOT NULL,
                    sha256 TEXT
                 );
                 CREATE TABLE audit_events (
                    audit_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    unix_time_secs INTEGER NOT NULL,
                    kind TEXT NOT NULL,
                    entity_type TEXT NOT NULL,
                    entity_id TEXT NOT NULL,
                    detail_json TEXT NOT NULL
                 );
                 PRAGMA user_version = 1;",
            )
            .unwrap();
        let controller = DurableController::from_connection(connection).unwrap();
        assert_eq!(controller.schema_version().unwrap(), 5);
        assert!(controller.recent_logs(10).unwrap().is_empty());
        assert!(controller.recent_metrics(10).unwrap().is_empty());
    }

    #[test]
    fn test_plans_persist_update_and_audit() {
        let mut controller = DurableController::open_in_memory().unwrap();
        let mut plan = TestPlan {
            version: TEST_PLAN_VERSION,
            name: "core-regression".into(),
            repository: RepositorySpec {
                url: "https://github.com/example/build.git".into(),
                revision: "main".into(),
            },
            steps: vec![PlanStep {
                id: "standard".into(),
                profile: PlanProfile::RustStandard,
                depends_on: vec![],
                condition: PlanCondition::DependenciesPassed,
                limits: Default::default(),
                required_capabilities: Default::default(),
                artifacts: vec![ArtifactKind::ExecutionReport],
                retry: RetryPolicy::no_retry(),
                target_os: TargetOs::Any,
                node_labels: Default::default(),
            }],
        };
        controller.upsert_test_plan(&plan, 1).unwrap();
        assert_eq!(
            controller.get_test_plan("core-regression").unwrap(),
            Some(plan.clone())
        );
        assert_eq!(
            controller.list_test_plans().unwrap(),
            vec!["core-regression".to_owned()]
        );

        plan.steps[0].artifacts.push(ArtifactKind::StepLogs);
        controller.upsert_test_plan(&plan, 2).unwrap();
        assert_eq!(
            controller
                .get_test_plan("core-regression")
                .unwrap()
                .unwrap()
                .steps[0]
                .artifacts
                .len(),
            2
        );
        let audit = controller
            .audit_events_for("test_plan", "core-regression")
            .unwrap();
        assert_eq!(audit.len(), 2);
        assert_eq!(audit[0].kind, "test_plan_created");
        assert_eq!(audit[1].kind, "test_plan_updated");
    }

    #[test]
    fn audit_chain_verifies_for_new_events() {
        let mut controller = DurableController::open_in_memory().unwrap();
        let job = test_job();
        controller.enqueue_job(&job, 10).unwrap();
        controller.register_worker(&test_worker(), 11).unwrap();
        controller.assign_next("windows-1", 12).unwrap().unwrap();
        assert!(controller.verify_audit_chain().unwrap());
        let events = controller
            .audit_events_for("job", &job.id.to_string())
            .unwrap();
        assert!(events.iter().all(|event| event.event_sha256.is_some()));
    }

    #[test]
    fn transient_infrastructure_failure_is_retried_after_due_time() {
        let mut controller = DurableController::open_in_memory().unwrap();
        let job = test_job();
        let policy = RetryPolicy::bounded(3, 10, 60, false).unwrap();
        controller.enqueue_job_with_retry(&job, policy, 1).unwrap();
        controller.register_worker(&test_worker(), 2).unwrap();
        controller.assign_next("windows-1", 3).unwrap().unwrap();
        controller.mark_running(job.id, 4).unwrap();

        let decision = controller
            .complete_job_with_classification(
                &JobResult {
                    job_id: job.id,
                    status: JobStatus::Failed,
                    summary: "worker transport dropped".into(),
                    artifacts: vec![],
                },
                Some(FailureClass::InfrastructureTransient),
                5,
            )
            .unwrap();
        assert_eq!(
            decision,
            LifecycleDecision::RetryScheduled {
                next_retry_at_secs: 15
            }
        );
        let pending = controller.get_job(job.id).unwrap().unwrap();
        assert_eq!(pending.state, DurableJobState::RetryPending);
        assert_eq!(pending.next_retry_at_secs, Some(15));
        assert!(controller.assign_next("windows-1", 14).unwrap().is_none());
        assert_eq!(
            controller.assign_next("windows-1", 15).unwrap().unwrap().id,
            job.id
        );
        assert_eq!(controller.list_attempts(job.id).unwrap().len(), 2);
    }

    #[test]
    fn test_failure_remains_terminal_even_with_retry_policy() {
        let mut controller = DurableController::open_in_memory().unwrap();
        let job = test_job();
        controller
            .enqueue_job_with_retry(&job, RetryPolicy::bounded(3, 1, 10, true).unwrap(), 1)
            .unwrap();
        controller.register_worker(&test_worker(), 2).unwrap();
        controller.assign_next("windows-1", 3).unwrap().unwrap();
        controller.mark_running(job.id, 4).unwrap();
        let decision = controller
            .complete_job_with_classification(
                &JobResult {
                    job_id: job.id,
                    status: JobStatus::Failed,
                    summary: "assertion failed".into(),
                    artifacts: vec![],
                },
                Some(FailureClass::TestFailure),
                5,
            )
            .unwrap();
        assert_eq!(decision, LifecycleDecision::TerminalFailed);
        assert_eq!(
            controller.get_job(job.id).unwrap().unwrap().state,
            DurableJobState::Failed
        );
    }

    #[test]
    fn interrupted_recovery_only_reschedules_when_policy_allows_it() {
        let mut controller = DurableController::open_in_memory().unwrap();
        let retry_job = test_job();
        let manual_job = test_job();
        controller
            .enqueue_job_with_retry(&retry_job, RetryPolicy::bounded(3, 5, 30, true).unwrap(), 1)
            .unwrap();
        controller.enqueue_job(&manual_job, 2).unwrap();
        controller.register_worker(&test_worker(), 3).unwrap();

        controller.assign_next("windows-1", 4).unwrap().unwrap();
        controller.mark_running(retry_job.id, 5).unwrap();
        controller.assign_next("windows-1", 6).unwrap().unwrap();
        controller.mark_running(manual_job.id, 7).unwrap();

        let interrupted = controller.recover_after_restart(10).unwrap();
        assert_eq!(interrupted.len(), 2);

        let retry = controller.get_job(retry_job.id).unwrap().unwrap();
        assert_eq!(retry.state, DurableJobState::RetryPending);
        assert_eq!(retry.next_retry_at_secs, Some(15));

        let manual = controller.get_job(manual_job.id).unwrap().unwrap();
        assert_eq!(manual.state, DurableJobState::Interrupted);
        assert!(manual.next_retry_at_secs.is_none());

        controller
            .reschedule_interrupted_job(manual_job.id, 11)
            .unwrap();
        assert_eq!(
            controller.get_job(manual_job.id).unwrap().unwrap().state,
            DurableJobState::Queued
        );
    }

    #[test]
    fn retry_pending_jobs_can_be_cancelled_before_reassignment() {
        let mut controller = DurableController::open_in_memory().unwrap();
        let job = test_job();
        controller
            .enqueue_job_with_retry(&job, RetryPolicy::bounded(3, 10, 60, false).unwrap(), 1)
            .unwrap();
        controller.register_worker(&test_worker(), 2).unwrap();
        controller.assign_next("windows-1", 3).unwrap().unwrap();
        controller.mark_running(job.id, 4).unwrap();
        controller
            .complete_job_with_classification(
                &JobResult {
                    job_id: job.id,
                    status: JobStatus::Failed,
                    summary: "temporary worker failure".into(),
                    artifacts: vec![],
                },
                Some(FailureClass::InfrastructureTransient),
                5,
            )
            .unwrap();
        assert_eq!(
            controller.get_job(job.id).unwrap().unwrap().state,
            DurableJobState::RetryPending
        );
        assert!(controller.cancel_queued_job(job.id, 6).unwrap());
        assert_eq!(
            controller.get_job(job.id).unwrap().unwrap().state,
            DurableJobState::Cancelled
        );
        assert!(controller.assign_next("windows-1", 100).unwrap().is_none());
    }

    #[test]
    fn retries_stop_at_persisted_attempt_limit() {
        let mut controller = DurableController::open_in_memory().unwrap();
        let job = test_job();
        controller
            .enqueue_job_with_retry(&job, RetryPolicy::bounded(2, 1, 10, false).unwrap(), 1)
            .unwrap();
        controller.register_worker(&test_worker(), 2).unwrap();

        for (assign_at, finish_at) in [(3, 4), (5, 6)] {
            controller
                .assign_next("windows-1", assign_at)
                .unwrap()
                .unwrap();
            controller.mark_running(job.id, assign_at).unwrap();
            let decision = controller
                .complete_job_with_classification(
                    &JobResult {
                        job_id: job.id,
                        status: JobStatus::Failed,
                        summary: "transient infrastructure fault".into(),
                        artifacts: vec![],
                    },
                    Some(FailureClass::InfrastructureTransient),
                    finish_at,
                )
                .unwrap();
            if finish_at == 4 {
                assert!(matches!(decision, LifecycleDecision::RetryScheduled { .. }));
            } else {
                assert_eq!(decision, LifecycleDecision::RetryExhausted);
            }
        }

        assert_eq!(
            controller.get_job(job.id).unwrap().unwrap().state,
            DurableJobState::Exhausted
        );
        assert_eq!(controller.list_attempts(job.id).unwrap().len(), 2);
    }

    #[test]
    fn manual_reschedule_stops_at_global_attempt_limit() {
        let mut controller = DurableController::open_in_memory().unwrap();
        let job = test_job();
        controller.enqueue_job(&job, 1).unwrap();
        controller.register_worker(&test_worker(), 2).unwrap();

        let mut now = 3;
        for attempt in 1..=df_test_lifecycle::MAX_ATTEMPTS {
            controller.assign_next("windows-1", now).unwrap().unwrap();
            controller.mark_running(job.id, now + 1).unwrap();
            controller.recover_after_restart(now + 2).unwrap();
            if attempt < df_test_lifecycle::MAX_ATTEMPTS {
                controller
                    .reschedule_interrupted_job(job.id, now + 3)
                    .unwrap();
            }
            now += 4;
        }

        assert!(matches!(
            controller.reschedule_interrupted_job(job.id, now),
            Err(DurableControllerError::ManualRescheduleLimitReached)
        ));
        assert_eq!(
            controller.list_attempts(job.id).unwrap().len(),
            df_test_lifecycle::MAX_ATTEMPTS as usize
        );
    }

    #[test]
    fn structured_logs_are_redacted_and_metrics_are_durable() {
        let mut controller = DurableController::open_in_memory().unwrap();
        let mut fields = std::collections::BTreeMap::new();
        fields.insert(
            "access_token".into(),
            serde_json::Value::String("secret-value".into()),
        );
        controller
            .record_log(&StructuredLogEvent {
                unix_time_secs: 20,
                level: LogLevel::Info,
                component: "controller".into(),
                message: "worker connected".into(),
                fields,
                job_id: None,
                worker_id: Some("windows-1".into()),
            })
            .unwrap();
        controller
            .record_metric(&MetricPoint {
                unix_time_secs: 21,
                name: "workers_online".into(),
                value: 1.0,
                labels: Default::default(),
            })
            .unwrap();

        let logs = controller.recent_logs(10).unwrap();
        assert_eq!(
            logs[0].event.fields.get("access_token").unwrap(),
            &serde_json::Value::String("<redacted>".into())
        );
        let metrics = controller.recent_metrics(10).unwrap();
        assert_eq!(metrics[0].point.name, "workers_online");
        let pruned = controller.prune_telemetry_before(30).unwrap();
        assert_eq!(pruned, (1, 1));
    }
}
