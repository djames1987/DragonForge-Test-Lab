use df_test_lifecycle::RetryPolicy;
use df_test_protocol::{Capability, JobRequest, RepositorySpec, ResourceLimits, TestAction};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub const TEST_PLAN_VERSION: u16 = 1;
pub const MAX_PLAN_STEPS: usize = 256;
pub const MAX_DEPENDENCIES_PER_STEP: usize = 64;
pub const MAX_ARTIFACTS_PER_STEP: usize = 32;
pub const MAX_NODE_LABELS: usize = 32;
pub const MAX_STEP_ID_BYTES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestPlan {
    pub version: u16,
    pub name: String,
    pub repository: RepositorySpec,
    pub steps: Vec<PlanStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub profile: PlanProfile,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub condition: PlanCondition,
    #[serde(default)]
    pub limits: ResourceLimits,
    #[serde(default)]
    pub required_capabilities: BTreeSet<Capability>,
    #[serde(default)]
    pub artifacts: Vec<ArtifactKind>,
    #[serde(default)]
    pub retry: RetryPolicy,
    #[serde(default)]
    pub target_os: TargetOs,
    #[serde(default)]
    pub node_labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlanProfile {
    RustFast,
    RustStandard,
    RustRelease,
    TypedActions { actions: Vec<TestAction> },
}

impl PlanProfile {
    pub fn actions(&self) -> Result<Vec<TestAction>, PlanError> {
        let actions = match self {
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
            Self::TypedActions { actions } => actions.clone(),
        };
        if actions.is_empty() || actions.len() > 32 {
            return Err(PlanError::InvalidProfileActions);
        }
        Ok(actions)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanCondition {
    #[default]
    DependenciesPassed,
    AlwaysAfterDependencies,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    StepLogs,
    ExecutionReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetOs {
    #[default]
    Any,
    Windows,
    Linux,
    Macos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStepStatus {
    Pending,
    Passed,
    Failed,
    Skipped,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledPlanStep {
    pub step_id: String,
    pub job: JobRequest,
    pub retry: RetryPolicy,
    pub condition: PlanCondition,
    pub target_os: TargetOs,
    pub node_labels: BTreeMap<String, String>,
    pub artifacts: Vec<ArtifactKind>,
}

impl CompiledPlanStep {
    pub fn matches_target(
        &self,
        worker_os: &str,
        labels: &BTreeMap<String, String>,
    ) -> bool {
        let os_matches = match self.target_os {
            TargetOs::Any => true,
            TargetOs::Windows => worker_os.eq_ignore_ascii_case("windows"),
            TargetOs::Linux => worker_os.eq_ignore_ascii_case("linux"),
            TargetOs::Macos => {
                worker_os.eq_ignore_ascii_case("macos")
                    || worker_os.eq_ignore_ascii_case("darwin")
            }
        };
        os_matches
            && self
                .node_labels
                .iter()
                .all(|(key, value)| labels.get(key) == Some(value))
    }
}

impl TestPlan {
    pub fn validate(&self) -> Result<(), PlanError> {
        if self.version != TEST_PLAN_VERSION {
            return Err(PlanError::UnsupportedVersion(self.version));
        }
        validate_identifier(&self.name, 128)?;
        if self.steps.is_empty() || self.steps.len() > MAX_PLAN_STEPS {
            return Err(PlanError::InvalidStepCount);
        }

        let ids = self
            .steps
            .iter()
            .map(|step| step.id.clone())
            .collect::<BTreeSet<_>>();
        if ids.len() != self.steps.len() {
            return Err(PlanError::DuplicateStepId);
        }

        for step in &self.steps {
            validate_identifier(&step.id, MAX_STEP_ID_BYTES)?;
            step.retry.validate()?;
            if step.depends_on.len() > MAX_DEPENDENCIES_PER_STEP
                || step.artifacts.len() > MAX_ARTIFACTS_PER_STEP
                || step.node_labels.len() > MAX_NODE_LABELS
            {
                return Err(PlanError::PlanBoundsExceeded);
            }
            validate_limits(&step.limits)?;
            let actions = step.profile.actions()?;
            let derived = actions
                .iter()
                .map(TestAction::required_capability)
                .collect::<BTreeSet<_>>();
            if !derived.is_subset(&step.required_capabilities) && !step.required_capabilities.is_empty()
            {
                return Err(PlanError::MissingDeclaredCapability);
            }
            for dep in &step.depends_on {
                if dep == &step.id || !ids.contains(dep) {
                    return Err(PlanError::InvalidDependency(dep.clone()));
                }
            }
            for (key, value) in &step.node_labels {
                validate_identifier(key, 64)?;
                validate_identifier(value, 128)?;
            }
        }

        self.topological_order()?;
        Ok(())
    }

    pub fn topological_order(&self) -> Result<Vec<String>, PlanError> {
        let mut remaining = self
            .steps
            .iter()
            .map(|step| (step.id.clone(), step.depends_on.iter().cloned().collect::<BTreeSet<_>>()))
            .collect::<BTreeMap<_, _>>();
        let mut ordered = Vec::with_capacity(remaining.len());

        while !remaining.is_empty() {
            let ready = remaining
                .iter()
                .filter(|(_, deps)| deps.is_empty())
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            if ready.is_empty() {
                return Err(PlanError::DependencyCycle);
            }
            for id in ready {
                remaining.remove(&id);
                for deps in remaining.values_mut() {
                    deps.remove(&id);
                }
                ordered.push(id);
            }
        }
        Ok(ordered)
    }

    pub fn compile_step(&self, step_id: &str) -> Result<CompiledPlanStep, PlanError> {
        self.validate()?;
        let step = self
            .steps
            .iter()
            .find(|step| step.id == step_id)
            .ok_or_else(|| PlanError::UnknownStep(step_id.to_owned()))?;
        let mut job = JobRequest::new(self.repository.clone(), step.profile.actions()?);
        job.extra_required_capabilities = step.required_capabilities.clone();
        job.limits = step.limits.clone();
        let derived = job.required_capabilities();
        if !step.required_capabilities.is_empty()
            && !derived.is_subset(&step.required_capabilities)
        {
            return Err(PlanError::MissingDeclaredCapability);
        }
        Ok(CompiledPlanStep {
            step_id: step.id.clone(),
            job,
            retry: step.retry,
            condition: step.condition,
            target_os: step.target_os,
            node_labels: step.node_labels.clone(),
            artifacts: step.artifacts.clone(),
        })
    }

    pub fn ready_steps(
        &self,
        statuses: &BTreeMap<String, PlanStepStatus>,
    ) -> Result<Vec<String>, PlanError> {
        self.validate()?;
        let mut ready = Vec::new();
        for id in self.topological_order()? {
            let step = self.steps.iter().find(|step| step.id == id).unwrap();
            if statuses.contains_key(&step.id) {
                continue;
            }
            let dep_statuses = step
                .depends_on
                .iter()
                .map(|dep| statuses.get(dep))
                .collect::<Vec<_>>();
            if dep_statuses.iter().any(|status| status.is_none()) {
                continue;
            }
            let can_run = match step.condition {
                PlanCondition::DependenciesPassed => dep_statuses
                    .iter()
                    .all(|status| matches!(status, Some(PlanStepStatus::Passed))),
                PlanCondition::AlwaysAfterDependencies => dep_statuses.iter().all(|status| {
                    matches!(
                        status,
                        Some(
                            PlanStepStatus::Passed
                                | PlanStepStatus::Failed
                                | PlanStepStatus::Skipped
                                | PlanStepStatus::Cancelled
                        )
                    )
                }),
            };
            if can_run {
                ready.push(step.id.clone());
            }
        }
        Ok(ready)
    }
}

fn validate_limits(limits: &ResourceLimits) -> Result<(), PlanError> {
    if limits.timeout_seconds == 0
        || limits.timeout_seconds > 86_400
        || limits.max_memory_mib < 128
        || limits.max_memory_mib > 131_072
        || limits.max_disk_mib < 128
        || limits.max_disk_mib > 1_048_576
        || limits.max_processes == 0
        || limits.max_processes > 4_096
    {
        return Err(PlanError::InvalidResourceLimits);
    }
    Ok(())
}

fn validate_identifier(value: &str, max: usize) -> Result<(), PlanError> {
    if value.is_empty()
        || value.len() > max
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(PlanError::InvalidIdentifier);
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum PlanError {
    #[error("unsupported test plan version: {0}")]
    UnsupportedVersion(u16),
    #[error("invalid plan step count")]
    InvalidStepCount,
    #[error("duplicate step id")]
    DuplicateStepId,
    #[error("invalid identifier")]
    InvalidIdentifier,
    #[error("plan bounds exceeded")]
    PlanBoundsExceeded,
    #[error("invalid dependency: {0}")]
    InvalidDependency(String),
    #[error("test plan dependency cycle")]
    DependencyCycle,
    #[error("invalid typed profile actions")]
    InvalidProfileActions,
    #[error("invalid plan resource limits")]
    InvalidResourceLimits,
    #[error("declared capability set does not cover typed actions")]
    MissingDeclaredCapability,
    #[error("unknown plan step: {0}")]
    UnknownStep(String),
    #[error("retry policy error: {0}")]
    Retry(#[from] df_test_lifecycle::LifecycleError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_plan() -> TestPlan {
        TestPlan {
            version: TEST_PLAN_VERSION,
            name: "phase16-sample".into(),
            repository: RepositorySpec {
                url: "https://github.com/djames1987/DragonForge-Test-Lab.git".into(),
                revision: "main".into(),
            },
            steps: vec![
                PlanStep {
                    id: "fast".into(),
                    profile: PlanProfile::RustFast,
                    depends_on: vec![],
                    condition: PlanCondition::DependenciesPassed,
                    limits: ResourceLimits::default(),
                    required_capabilities: BTreeSet::new(),
                    artifacts: vec![ArtifactKind::StepLogs],
                    retry: RetryPolicy::no_retry(),
                    target_os: TargetOs::Any,
                    node_labels: BTreeMap::new(),
                },
                PlanStep {
                    id: "standard".into(),
                    profile: PlanProfile::RustStandard,
                    depends_on: vec!["fast".into()],
                    condition: PlanCondition::DependenciesPassed,
                    limits: ResourceLimits::default(),
                    required_capabilities: BTreeSet::new(),
                    artifacts: vec![ArtifactKind::ExecutionReport],
                    retry: RetryPolicy::bounded(2, 5, 30, false).unwrap(),
                    target_os: TargetOs::Windows,
                    node_labels: BTreeMap::from([("tier".into(), "primary".into())]),
                },
            ],
        }
    }

    #[test]
    fn plan_validates_and_orders_dependencies() {
        let plan = sample_plan();
        plan.validate().unwrap();
        assert_eq!(plan.topological_order().unwrap(), vec!["fast", "standard"]);
    }

    #[test]
    fn cycles_are_rejected() {
        let mut plan = sample_plan();
        plan.steps[0].depends_on = vec!["standard".into()];
        assert!(matches!(plan.validate(), Err(PlanError::DependencyCycle)));
    }

    #[test]
    fn profile_compiles_only_to_typed_actions() {
        let step = sample_plan().compile_step("standard").unwrap();
        assert!(step
            .job
            .actions
            .iter()
            .all(|action| !format!("{action:?}").is_empty()));
        assert!(step.job.required_capabilities().contains(&Capability::CargoTest));
        assert!(step
            .job
            .required_capabilities()
            .contains(&Capability::CargoClippy));
        assert_eq!(step.retry.max_attempts, 2);
    }

    #[test]
    fn target_os_and_labels_are_enforced() {
        let step = sample_plan().compile_step("standard").unwrap();
        let matching = BTreeMap::from([("tier".into(), "primary".into())]);
        let wrong = BTreeMap::from([("tier".into(), "secondary".into())]);
        assert!(step.matches_target("windows", &matching));
        assert!(!step.matches_target("linux", &matching));
        assert!(!step.matches_target("windows", &wrong));
    }

    #[test]
    fn ready_steps_honor_dependencies_and_conditions() {
        let plan = sample_plan();
        assert_eq!(plan.ready_steps(&BTreeMap::new()).unwrap(), vec!["fast"]);
        let statuses = BTreeMap::from([("fast".into(), PlanStepStatus::Passed)]);
        assert_eq!(plan.ready_steps(&statuses).unwrap(), vec!["standard"]);
    }

    #[test]
    fn failed_dependency_blocks_default_condition() {
        let plan = sample_plan();
        let statuses = BTreeMap::from([("fast".into(), PlanStepStatus::Failed)]);
        assert!(plan.ready_steps(&statuses).unwrap().is_empty());
    }
}
