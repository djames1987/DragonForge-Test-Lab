use df_test_lifecycle::{decide_failure, FailureClass, LifecycleDecision, RetryPolicy};
use df_test_observability::{next_audit_digest, LogLevel, MetricPoint, StructuredLogEvent};
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
                 next_retry_at_secs = NULL
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
                    LifecycleDecision::RetryScheduled {
                        next_retry_at_secs,
                    } => (
                        decision,
                        DurableJobState::RetryPending,
                        Some(class),
                        Some(next_retry_at_secs),
                    ),
                    LifecycleDecision::RetryExhausted => (
                        decision,
                        DurableJobState::Exhausted,
                        Some(class),
                        None,
                    ),
                    LifecycleDecision::TerminalRejected => (
                        decision,
                        DurableJobState::Rejected,
                        Some(class),
                        None,
                    ),
                    LifecycleDecision::TerminalCancelled => (
                        decision,
                        DurableJobState::Cancelled,
                        Some(class),
                        None,
                    ),
                    LifecycleDecision::InterruptedAwaitingDecision => (
                        decision,
                        DurableJobState::Interrupted,
                        Some(class),
                        None,
                    ),
                    _ => (
                        decision,
                        DurableJobState::Failed,
                        Some(class),
                        None,
                    ),
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
                if matches!(final_state, DurableJobState::Failed | DurableJobState::Exhausted | DurableJobState::Interrupted | DurableJobState::RetryPending) {
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
                attempt_terminal_state(result.status, final_state),
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
        let changed = tx.execute(
            "UPDATE jobs SET state = 'cancelled', updated_at_secs = ?2
             WHERE job_id = ?1 AND state = 'queued'",
            params![job_id.to_string(), to_i64(now_secs)?],
        )?;
        if changed > 0 {
            insert_audit(
                &tx,
                now_secs,
                "job_cancelled",
                "job",
                &job_id.to_string(),
                &serde_json::json!({"from":"queued"}),
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
                LifecycleDecision::RetryScheduled {
                    next_retry_at_secs,
                } => (DurableJobState::RetryPending, Some(next_retry_at_secs)),
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
        let changed = tx.execute(
            "UPDATE jobs
             SET state = 'queued',
                 assigned_worker = NULL,
                 updated_at_secs = ?2,
                 failure_class = NULL,
                 next_retry_at_secs = NULL,
                 retry_reason = NULL
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
                        created_at_secs, updated_at_secs, last_error
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
                    ))
                },
            )
            .optional()?;

        row.map(
            |(job_json, state, assigned_worker, created, updated, last_error)| {
                Ok(DurableJobRecord {
                    job: serde_json::from_str(&job_json)?,
                    state: DurableJobState::parse(&state)?,
                    assigned_worker,
                    created_at_secs: to_u64(created)?,
                    updated_at_secs: to_u64(updated)?,
                    last_error,
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
                    started_at_secs, finished_at_secs, summary
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
            ))
        })?;

        let mut attempts = Vec::new();
        for row in rows {
            let (attempt_id, attempt_number, worker_id, state, started, finished, summary) = row?;
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
    #[error("invalid durable job state transition")]
    InvalidTransition,
    #[error("completion result does not contain a terminal status")]
    InvalidCompletionStatus,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use df_test_observability::{LogLevel, MetricPoint, StructuredLogEvent};
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
    fn schema_v1_migrates_to_v2_observability_tables() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE artifact_metadata (
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
        assert_eq!(controller.schema_version().unwrap(), 2);
        assert!(controller.recent_logs(10).unwrap().is_empty());
        assert!(controller.recent_metrics(10).unwrap().is_empty());
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
