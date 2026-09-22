use df_test_controller::DurableController;
use df_test_github::{GhGitHubClient, GitComparison, GitHubError, GitHubRepository};
use df_test_intelligence::{analyze, ChangeSet, IntelligenceInput, IntelligenceReport, TestProfile};
use df_test_plans::{PlanProfile, TargetOs, TestPlan};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const DEFAULT_MIN_AUTOMATIC_SCORE: u32 = 60;
pub const MAX_HISTORY_INPUT: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntelligenceMode {
    Advisory,
    Automatic,
}

impl IntelligenceMode {
    pub fn parse(value: &str) -> Result<Self, IntegrationError> {
        match value {
            "advisory" => Ok(Self::Advisory),
            "automatic" => Ok(Self::Automatic),
            _ => Err(IntegrationError::InvalidMode),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Advisory => "advisory",
            Self::Automatic => "automatic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrationRequest {
    pub repository_url: String,
    pub base_revision: String,
    pub head_revision: String,
    pub plan_name: String,
    pub mode: IntelligenceMode,
    pub min_automatic_score: u32,
    pub now_secs: u64,
}

impl IntegrationRequest {
    pub fn validate(&self) -> Result<(), IntegrationError> {
        GitHubRepository::parse_https(&self.repository_url)?;
        if self.plan_name.is_empty() || self.plan_name.len() > 128 {
            return Err(IntegrationError::InvalidPlanName);
        }
        if !(1..=100).contains(&self.min_automatic_score) {
            return Err(IntegrationError::InvalidScoreThreshold);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedPlanStep {
    pub step_id: String,
    pub profile: TestProfile,
    pub score: u32,
    pub queued_job_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationSkip {
    pub step_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntelligenceDecision {
    pub decision_id: Uuid,
    pub mode: IntelligenceMode,
    pub repository: String,
    pub base_sha: String,
    pub head_sha: String,
    pub changed_files: Vec<String>,
    pub historical_failures_used: usize,
    pub live_workers_used: usize,
    pub intelligence: IntelligenceReport,
    pub plan_name: String,
    pub selected_steps: Vec<SelectedPlanStep>,
    pub automation_skips: Vec<AutomationSkip>,
}

pub trait ChangeSource {
    fn compare(
        &self,
        repository: &GitHubRepository,
        base_revision: &str,
        head_revision: &str,
    ) -> Result<GitComparison, IntegrationError>;
}

impl ChangeSource for GhGitHubClient {
    fn compare(
        &self,
        repository: &GitHubRepository,
        base_revision: &str,
        head_revision: &str,
    ) -> Result<GitComparison, IntegrationError> {
        Ok(self.compare_changes(repository, base_revision, head_revision)?)
    }
}

pub fn integrate<S: ChangeSource>(
    controller: &mut DurableController,
    source: &S,
    request: &IntegrationRequest,
) -> Result<IntelligenceDecision, IntegrationError> {
    request.validate()?;
    let repository = GitHubRepository::parse_https(&request.repository_url)?;
    let comparison = source.compare(
        &repository,
        &request.base_revision,
        &request.head_revision,
    )?;
    let history = controller.intelligence_historical_failures(MAX_HISTORY_INPUT)?;
    let workers = controller.intelligence_worker_capacities()?;
    let input = IntelligenceInput {
        changes: ChangeSet {
            files: comparison.files.clone(),
        },
        history: history.clone(),
        workers: workers.clone(),
    };
    let intelligence = analyze(&input)?;
    let plan = controller
        .get_test_plan(&request.plan_name)?
        .ok_or_else(|| IntegrationError::PlanNotFound(request.plan_name.clone()))?;
    validate_plan_repository(&plan, &request.repository_url)?;

    let mut selected_steps = Vec::new();
    let mut automation_skips = Vec::new();

    for recommendation in &intelligence.recommendations {
        let matching_steps = plan
            .steps
            .iter()
            .filter(|step| profile_for_plan(&step.profile) == Some(recommendation.profile))
            .collect::<Vec<_>>();
        for step in matching_steps {
            if recommendation.score < request.min_automatic_score {
                automation_skips.push(AutomationSkip {
                    step_id: step.id.clone(),
                    reason: format!(
                        "score {} is below threshold {}",
                        recommendation.score, request.min_automatic_score
                    ),
                });
                continue;
            }

            let mut selected = SelectedPlanStep {
                step_id: step.id.clone(),
                profile: recommendation.profile,
                score: recommendation.score,
                queued_job_id: None,
            };

            if request.mode == IntelligenceMode::Automatic {
                let scheduled = intelligence
                    .schedule
                    .iter()
                    .any(|item| item.profile == recommendation.profile);
                if !scheduled {
                    automation_skips.push(AutomationSkip {
                        step_id: step.id.clone(),
                        reason: "no live eligible worker capacity".into(),
                    });
                } else if !step.depends_on.is_empty() {
                    automation_skips.push(AutomationSkip {
                        step_id: step.id.clone(),
                        reason: "automatic mode does not bypass plan dependencies".into(),
                    });
                } else if step.target_os != TargetOs::Any || !step.node_labels.is_empty() {
                    automation_skips.push(AutomationSkip {
                        step_id: step.id.clone(),
                        reason: "automatic mode requires unconstrained target routing".into(),
                    });
                } else {
                    let mut compiled = plan.compile_step(&step.id)?;
                    compiled.job.repository.revision = comparison.head_sha.clone();
                    controller.enqueue_job_with_retry(
                        &compiled.job,
                        compiled.retry,
                        request.now_secs,
                    )?;
                    controller.record_intelligence_job_context(
                        compiled.job.id,
                        recommendation.profile,
                        &comparison.files,
                        request.now_secs,
                    )?;
                    selected.queued_job_id = Some(compiled.job.id);
                }
            }
            selected_steps.push(selected);
        }
    }

    let decision_id = Uuid::new_v4();
    let decision = IntelligenceDecision {
        decision_id,
        mode: request.mode,
        repository: repository.slug(),
        base_sha: comparison.base_sha,
        head_sha: comparison.head_sha,
        changed_files: comparison.files,
        historical_failures_used: history.len(),
        live_workers_used: workers.len(),
        intelligence,
        plan_name: request.plan_name.clone(),
        selected_steps,
        automation_skips,
    };
    controller.record_intelligence_decision(
        decision_id,
        request.mode.as_str(),
        &serde_json::to_value(&decision)?,
        request.now_secs,
    )?;
    Ok(decision)
}

fn validate_plan_repository(plan: &TestPlan, repository_url: &str) -> Result<(), IntegrationError> {
    if plan.repository.url != repository_url {
        return Err(IntegrationError::PlanRepositoryMismatch);
    }
    Ok(())
}

fn profile_for_plan(profile: &PlanProfile) -> Option<TestProfile> {
    match profile {
        PlanProfile::RustFast => Some(TestProfile::RustFast),
        PlanProfile::RustStandard => Some(TestProfile::RustStandard),
        PlanProfile::RustRelease | PlanProfile::TypedActions { .. } => None,
    }
}

#[derive(Debug, Error)]
pub enum IntegrationError {
    #[error("invalid intelligence integration mode")]
    InvalidMode,
    #[error("plan name is invalid")]
    InvalidPlanName,
    #[error("automatic score threshold must be between 1 and 100")]
    InvalidScoreThreshold,
    #[error("stored test plan was not found: {0}")]
    PlanNotFound(String),
    #[error("stored plan repository does not match the requested repository")]
    PlanRepositoryMismatch,
    #[error(transparent)]
    GitHub(#[from] GitHubError),
    #[error(transparent)]
    Intelligence(#[from] df_test_intelligence::IntelligenceError),
    #[error(transparent)]
    Controller(#[from] df_test_controller::DurableControllerError),
    #[error(transparent)]
    Plan(#[from] df_test_plans::PlanError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use df_test_lifecycle::FailureClass;
    use df_test_plans::{
        ArtifactKind, PlanCondition, PlanStep, TEST_PLAN_VERSION,
    };
    use df_test_protocol::{
        Capability, JobRequest, JobResult, JobStatus, RepositorySpec, ResourceLimits, TestAction,
        WorkerRegistration, PROTOCOL_VERSION,
    };
    use std::collections::{BTreeMap, BTreeSet};

    struct FakeChanges;

    impl ChangeSource for FakeChanges {
        fn compare(
            &self,
            _repository: &GitHubRepository,
            _base_revision: &str,
            _head_revision: &str,
        ) -> Result<GitComparison, IntegrationError> {
            Ok(GitComparison {
                base_sha: "1111111111111111111111111111111111111111".into(),
                head_sha: "2222222222222222222222222222222222222222".into(),
                files: vec!["crates/df-test-controller/src/lib.rs".into()],
            })
        }
    }

    fn plan() -> TestPlan {
        TestPlan {
            version: TEST_PLAN_VERSION,
            name: "phase17".into(),
            repository: RepositorySpec {
                url: "https://github.com/djames1987/DragonForge-Test-Lab.git".into(),
                revision: "main".into(),
            },
            steps: vec![PlanStep {
                id: "standard".into(),
                profile: PlanProfile::RustStandard,
                depends_on: vec![],
                condition: PlanCondition::DependenciesPassed,
                limits: ResourceLimits::default(),
                required_capabilities: BTreeSet::new(),
                artifacts: vec![ArtifactKind::ExecutionReport],
                retry: df_test_lifecycle::RetryPolicy::no_retry(),
                target_os: TargetOs::Any,
                node_labels: BTreeMap::new(),
            }],
        }
    }

    fn worker() -> WorkerRegistration {
        WorkerRegistration {
            worker_id: "phase17-worker".into(),
            protocol_version: PROTOCOL_VERSION,
            os: "windows".into(),
            arch: "x86_64".into(),
            capabilities: [
                Capability::CheckoutRepository,
                Capability::CargoFmtCheck,
                Capability::CargoClippy,
                Capability::CargoTest,
            ]
            .into_iter()
            .collect(),
        }
    }

    #[test]
    fn automatic_mode_uses_history_capacity_and_queues_typed_plan_job() {
        let mut controller = DurableController::open_in_memory().unwrap();
        controller.upsert_test_plan(&plan(), 1).unwrap();
        controller.register_worker(&worker(), 2).unwrap();

        let historical = JobRequest::new(
            RepositorySpec {
                url: "https://github.com/djames1987/DragonForge-Test-Lab.git".into(),
                revision: "old".into(),
            },
            vec![
                TestAction::Checkout,
                TestAction::CargoFmtCheck,
                TestAction::CargoClippy { deny_warnings: true },
                TestAction::CargoTest { all_features: true },
            ],
        );
        controller.enqueue_job(&historical, 3).unwrap();
        controller
            .record_intelligence_job_context(
                historical.id,
                TestProfile::RustStandard,
                &["crates/df-test-controller/src/lib.rs".into()],
                3,
            )
            .unwrap();
        controller.assign_next("phase17-worker", 4).unwrap().unwrap();
        controller.mark_running(historical.id, 5).unwrap();
        controller
            .complete_job_with_classification(
                &JobResult {
                    job_id: historical.id,
                    status: JobStatus::Failed,
                    summary: "controller regression".into(),
                    artifacts: vec![],
                },
                Some(FailureClass::TestFailure),
                6,
            )
            .unwrap();

        let decision = integrate(
            &mut controller,
            &FakeChanges,
            &IntegrationRequest {
                repository_url: "https://github.com/djames1987/DragonForge-Test-Lab.git".into(),
                base_revision: "main".into(),
                head_revision: "feature".into(),
                plan_name: "phase17".into(),
                mode: IntelligenceMode::Automatic,
                min_automatic_score: 35,
                now_secs: 10,
            },
        )
        .unwrap();

        assert_eq!(decision.historical_failures_used, 1);
        assert_eq!(decision.live_workers_used, 1);
        assert!(decision
            .selected_steps
            .iter()
            .any(|step| step.queued_job_id.is_some()));
        let audit = controller
            .audit_events_for("intelligence_decision", &decision.decision_id.to_string())
            .unwrap();
        assert_eq!(audit.len(), 1);
        assert!(controller.verify_audit_chain().unwrap());
    }

    #[test]
    fn advisory_mode_never_enqueues() {
        let mut controller = DurableController::open_in_memory().unwrap();
        controller.upsert_test_plan(&plan(), 1).unwrap();
        controller.register_worker(&worker(), 2).unwrap();
        let decision = integrate(
            &mut controller,
            &FakeChanges,
            &IntegrationRequest {
                repository_url: "https://github.com/djames1987/DragonForge-Test-Lab.git".into(),
                base_revision: "main".into(),
                head_revision: "feature".into(),
                plan_name: "phase17".into(),
                mode: IntelligenceMode::Advisory,
                min_automatic_score: 20,
                now_secs: 10,
            },
        )
        .unwrap();
        assert!(decision
            .selected_steps
            .iter()
            .all(|step| step.queued_job_id.is_none()));
    }
}
