use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_CHANGED_FILES: usize = 4096;
pub const MAX_HISTORY_RECORDS: usize = 10_000;
pub const MAX_FAILURE_TEXT: usize = 16_384;
pub const MAX_WORKERS: usize = 1024;
pub const MAX_TASKS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestProfile {
    RustFast,
    RustStandard,
    RustDeep,
    WindowsIntegration,
    GuiAutomation,
    DistributedNetwork,
    McpGateway,
    FullRegression,
}

impl TestProfile {
    pub fn estimated_memory_mib(self) -> u64 {
        match self {
            Self::RustFast => 1024,
            Self::RustStandard => 2048,
            Self::RustDeep => 4096,
            Self::WindowsIntegration => 2048,
            Self::GuiAutomation => 2048,
            Self::DistributedNetwork => 1536,
            Self::McpGateway => 2048,
            Self::FullRegression => 4096,
        }
    }

    pub fn estimated_weight(self) -> u32 {
        match self {
            Self::RustFast => 10,
            Self::RustStandard => 25,
            Self::RustDeep => 70,
            Self::WindowsIntegration => 35,
            Self::GuiAutomation => 40,
            Self::DistributedNetwork => 45,
            Self::McpGateway => 35,
            Self::FullRegression => 100,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSet {
    pub files: Vec<String>,
}

impl ChangeSet {
    pub fn validate(&self) -> Result<(), IntelligenceError> {
        if self.files.len() > MAX_CHANGED_FILES {
            return Err(IntelligenceError::TooManyChangedFiles);
        }
        for file in &self.files {
            validate_repo_path(file)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileRecommendation {
    pub profile: TestProfile,
    pub score: u32,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoricalFailure {
    pub id: Uuid,
    pub profile: TestProfile,
    pub step: String,
    pub message: String,
    pub changed_files: Vec<String>,
    pub unix_time_secs: u64,
}

impl HistoricalFailure {
    pub fn validate(&self) -> Result<(), IntelligenceError> {
        if self.step.is_empty() || self.step.len() > 256 {
            return Err(IntelligenceError::InvalidFailureRecord);
        }
        if self.message.is_empty() || self.message.len() > MAX_FAILURE_TEXT {
            return Err(IntelligenceError::InvalidFailureRecord);
        }
        if self.changed_files.len() > MAX_CHANGED_FILES {
            return Err(IntelligenceError::InvalidFailureRecord);
        }
        for path in &self.changed_files {
            validate_repo_path(path)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureCluster {
    pub fingerprint: String,
    pub normalized_signature: String,
    pub profiles: BTreeSet<TestProfile>,
    pub occurrences: usize,
    pub first_seen_secs: u64,
    pub last_seen_secs: u64,
    pub sample_failure_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerCapacity {
    pub worker_id: String,
    pub supported_profiles: BTreeSet<TestProfile>,
    pub total_memory_mib: u64,
    pub free_memory_mib: u64,
    pub max_parallel_jobs: u16,
    pub active_jobs: u16,
    pub load_percent: u8,
}

impl WorkerCapacity {
    pub fn validate(&self) -> Result<(), IntelligenceError> {
        validate_identifier(&self.worker_id, 96)?;
        if self.total_memory_mib == 0
            || self.free_memory_mib > self.total_memory_mib
            || self.max_parallel_jobs == 0
            || self.active_jobs > self.max_parallel_jobs
            || self.load_percent > 100
        {
            return Err(IntelligenceError::InvalidWorkerCapacity);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledProfile {
    pub profile: TestProfile,
    pub worker_id: String,
    pub score: u32,
    pub estimated_memory_mib: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntelligenceInput {
    pub changes: ChangeSet,
    #[serde(default)]
    pub history: Vec<HistoricalFailure>,
    #[serde(default)]
    pub workers: Vec<WorkerCapacity>,
}

impl IntelligenceInput {
    pub fn validate(&self) -> Result<(), IntelligenceError> {
        self.changes.validate()?;
        if self.history.len() > MAX_HISTORY_RECORDS {
            return Err(IntelligenceError::TooMuchHistory);
        }
        if self.workers.len() > MAX_WORKERS {
            return Err(IntelligenceError::TooManyWorkers);
        }
        for record in &self.history {
            record.validate()?;
        }
        for worker in &self.workers {
            worker.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntelligenceReport {
    pub recommendations: Vec<ProfileRecommendation>,
    pub regression_targets: Vec<TestProfile>,
    pub failure_clusters: Vec<FailureCluster>,
    pub schedule: Vec<ScheduledProfile>,
    pub unscheduled_profiles: Vec<TestProfile>,
}

pub fn analyze(input: &IntelligenceInput) -> Result<IntelligenceReport, IntelligenceError> {
    input.validate()?;
    let mut recommendations = recommend_profiles(&input.changes)?;
    apply_history_scores(&mut recommendations, &input.changes, &input.history);
    recommendations.sort_by(|a, b| b.score.cmp(&a.score).then(a.profile.cmp(&b.profile)));

    let regression_targets = recommendations
        .iter()
        .filter(|item| item.score >= 35)
        .map(|item| item.profile)
        .collect::<Vec<_>>();

    let failure_clusters = cluster_failures(&input.history)?;
    let (schedule, unscheduled_profiles) = schedule_profiles(&recommendations, &input.workers)?;

    Ok(IntelligenceReport {
        recommendations,
        regression_targets,
        failure_clusters,
        schedule,
        unscheduled_profiles,
    })
}

pub fn recommend_profiles(changes: &ChangeSet) -> Result<Vec<ProfileRecommendation>, IntelligenceError> {
    changes.validate()?;
    let mut scores: BTreeMap<TestProfile, (u32, BTreeSet<String>)> = BTreeMap::new();

    if changes.files.is_empty() {
        add_score(&mut scores, TestProfile::RustFast, 10, "no changed files supplied");
    }

    for path in &changes.files {
        let lower = path.to_ascii_lowercase();

        add_score(
            &mut scores,
            TestProfile::RustFast,
            10,
            format!("change detected: {path}"),
        );

        if lower == "cargo.toml" || lower == "cargo.lock" || lower.ends_with("/cargo.toml") {
            add_score(
                &mut scores,
                TestProfile::RustStandard,
                30,
                format!("Rust dependency/workspace metadata changed: {path}"),
            );
        }

        if lower.starts_with("crates/df-test-protocol/")
            || lower.starts_with("crates/df-test-policy/")
            || lower.starts_with("crates/df-test-agent/")
            || lower.starts_with("crates/df-test-executor/")
            || lower.starts_with("crates/df-test-controller/")
        {
            add_score(
                &mut scores,
                TestProfile::RustStandard,
                35,
                format!("core execution/trust boundary changed: {path}"),
            );
            add_score(
                &mut scores,
                TestProfile::RustDeep,
                25,
                format!("core contract change merits deeper regression: {path}"),
            );
        }

        if lower.starts_with("crates/df-test-windows/") {
            add_score(
                &mut scores,
                TestProfile::WindowsIntegration,
                60,
                format!("Windows integration code changed: {path}"),
            );
        }
        if lower.starts_with("crates/df-test-gui/") || lower.contains("phase7-gui") {
            add_score(
                &mut scores,
                TestProfile::GuiAutomation,
                65,
                format!("GUI automation code changed: {path}"),
            );
        }
        if lower.starts_with("crates/df-test-distributed/") || lower.contains("phase8") {
            add_score(
                &mut scores,
                TestProfile::DistributedNetwork,
                65,
                format!("distributed/network code changed: {path}"),
            );
        }
        if lower.starts_with("crates/df-test-mcp/") || lower.contains("phase9") {
            add_score(
                &mut scores,
                TestProfile::McpGateway,
                65,
                format!("MCP gateway code changed: {path}"),
            );
        }
        if lower.starts_with("scripts/") {
            add_score(
                &mut scores,
                TestProfile::RustStandard,
                15,
                format!("validation script changed: {path}"),
            );
        }
        if lower.starts_with("docs/") || lower == "readme.md" {
            add_score(
                &mut scores,
                TestProfile::RustFast,
                5,
                format!("documentation changed: {path}"),
            );
        }
    }

    let cross_cutting = changes.files.iter().filter(|path| path.starts_with("crates/")).count();
    if cross_cutting >= 4 {
        add_score(
            &mut scores,
            TestProfile::FullRegression,
            60,
            "changes span four or more crates",
        );
    }

    let mut result = scores
        .into_iter()
        .map(|(profile, (score, reasons))| ProfileRecommendation {
            profile,
            score: score.min(100),
            reasons: reasons.into_iter().collect(),
        })
        .collect::<Vec<_>>();
    result.sort_by(|a, b| b.score.cmp(&a.score).then(a.profile.cmp(&b.profile)));
    Ok(result)
}

pub fn cluster_failures(history: &[HistoricalFailure]) -> Result<Vec<FailureCluster>, IntelligenceError> {
    if history.len() > MAX_HISTORY_RECORDS {
        return Err(IntelligenceError::TooMuchHistory);
    }
    let mut clusters: BTreeMap<String, FailureCluster> = BTreeMap::new();

    for failure in history {
        failure.validate()?;
        let normalized = normalize_failure_signature(&failure.step, &failure.message);
        let fingerprint = hex::encode(Sha256::digest(normalized.as_bytes()));
        let entry = clusters.entry(fingerprint.clone()).or_insert_with(|| FailureCluster {
            fingerprint,
            normalized_signature: normalized,
            profiles: BTreeSet::new(),
            occurrences: 0,
            first_seen_secs: failure.unix_time_secs,
            last_seen_secs: failure.unix_time_secs,
            sample_failure_ids: Vec::new(),
        });
        entry.profiles.insert(failure.profile);
        entry.occurrences += 1;
        entry.first_seen_secs = entry.first_seen_secs.min(failure.unix_time_secs);
        entry.last_seen_secs = entry.last_seen_secs.max(failure.unix_time_secs);
        if entry.sample_failure_ids.len() < 5 {
            entry.sample_failure_ids.push(failure.id);
        }
    }

    let mut result = clusters.into_values().collect::<Vec<_>>();
    result.sort_by(|a, b| {
        b.occurrences
            .cmp(&a.occurrences)
            .then(b.last_seen_secs.cmp(&a.last_seen_secs))
            .then(a.fingerprint.cmp(&b.fingerprint))
    });
    Ok(result)
}

pub fn schedule_profiles(
    recommendations: &[ProfileRecommendation],
    workers: &[WorkerCapacity],
) -> Result<(Vec<ScheduledProfile>, Vec<TestProfile>), IntelligenceError> {
    if recommendations.len() > MAX_TASKS {
        return Err(IntelligenceError::TooManyTasks);
    }
    if workers.len() > MAX_WORKERS {
        return Err(IntelligenceError::TooManyWorkers);
    }
    for worker in workers {
        worker.validate()?;
    }

    let mut mutable = workers
        .iter()
        .map(|worker| (worker.clone(), worker.free_memory_mib, worker.active_jobs))
        .collect::<Vec<_>>();
    let mut scheduled = Vec::new();
    let mut unscheduled = Vec::new();

    for recommendation in recommendations.iter().filter(|item| item.score >= 20) {
        let required_memory = recommendation.profile.estimated_memory_mib();
        let mut candidates = mutable
            .iter()
            .enumerate()
            .filter(|(_, (worker, free_memory, active_jobs))| {
                worker.supported_profiles.contains(&recommendation.profile)
                    && *free_memory >= required_memory
                    && *active_jobs < worker.max_parallel_jobs
            })
            .map(|(index, (worker, free_memory, active_jobs))| {
                (
                    index,
                    worker.load_percent,
                    *active_jobs,
                    std::cmp::Reverse(*free_memory),
                    worker.worker_id.clone(),
                )
            })
            .collect::<Vec<_>>();

        candidates.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then(a.2.cmp(&b.2))
                .then(a.3.cmp(&b.3))
                .then(a.4.cmp(&b.4))
        });

        if let Some((index, _, _, _, worker_id)) = candidates.into_iter().next() {
            let (_, free_memory, active_jobs) = &mut mutable[index];
            *free_memory = free_memory.saturating_sub(required_memory);
            *active_jobs = active_jobs.saturating_add(1);
            scheduled.push(ScheduledProfile {
                profile: recommendation.profile,
                worker_id,
                score: recommendation.score,
                estimated_memory_mib: required_memory,
            });
        } else {
            unscheduled.push(recommendation.profile);
        }
    }

    Ok((scheduled, unscheduled))
}

fn apply_history_scores(
    recommendations: &mut Vec<ProfileRecommendation>,
    changes: &ChangeSet,
    history: &[HistoricalFailure],
) {
    let changed: BTreeSet<&str> = changes.files.iter().map(String::as_str).collect();
    let mut bonuses: BTreeMap<TestProfile, (u32, BTreeSet<String>)> = BTreeMap::new();

    for failure in history {
        let overlap = failure
            .changed_files
            .iter()
            .filter(|path| changed.contains(path.as_str()))
            .count();
        if overlap > 0 {
            let bonus = (overlap as u32).saturating_mul(12).min(36);
            add_score(
                &mut bonuses,
                failure.profile,
                bonus,
                format!(
                    "historical failure {} overlaps {overlap} changed file(s)",
                    failure.id
                ),
            );
        }
    }

    for (profile, (bonus, reasons)) in bonuses {
        if let Some(item) = recommendations.iter_mut().find(|item| item.profile == profile) {
            item.score = item.score.saturating_add(bonus).min(100);
            item.reasons.extend(reasons);
            item.reasons.sort();
            item.reasons.dedup();
        } else {
            recommendations.push(ProfileRecommendation {
                profile,
                score: bonus.min(100),
                reasons: reasons.into_iter().collect(),
            });
        }
    }
}

fn add_score(
    scores: &mut BTreeMap<TestProfile, (u32, BTreeSet<String>)>,
    profile: TestProfile,
    score: u32,
    reason: impl Into<String>,
) {
    let entry = scores.entry(profile).or_insert_with(|| (0, BTreeSet::new()));
    entry.0 = entry.0.saturating_add(score).min(100);
    entry.1.insert(reason.into());
}

fn normalize_failure_signature(step: &str, message: &str) -> String {
    let combined = format!("{}|{}", step.trim().to_ascii_lowercase(), message.trim().to_ascii_lowercase());
    let mut output = String::with_capacity(combined.len());
    let mut token = String::new();

    for ch in combined.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':' | '/' | '\\') {
            token.push(ch);
        } else {
            flush_normalized_token(&mut output, &mut token);
            if !output.ends_with(' ') {
                output.push(' ');
            }
        }
    }
    flush_normalized_token(&mut output, &mut token);
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn flush_normalized_token(output: &mut String, token: &mut String) {
    if token.is_empty() {
        return;
    }
    let normalized = if looks_volatile(token) {
        "<volatile>"
    } else {
        token.as_str()
    };
    if !output.is_empty() && !output.ends_with(' ') {
        output.push(' ');
    }
    output.push_str(normalized);
    token.clear();
}

fn looks_volatile(token: &str) -> bool {
    let digits = token.bytes().filter(|byte| byte.is_ascii_digit()).count();
    let hex_chars = token.bytes().filter(|byte| byte.is_ascii_hexdigit()).count();
    let path_like = token.contains('\\') || token.contains('/');
    let uuid_like = token.len() >= 32 && token.contains('-') && hex_chars + token.matches('-').count() >= token.len();
    let long_number = token.len() >= 4 && digits == token.len();
    let long_hex = token.len() >= 8 && hex_chars == token.len();
    path_like || uuid_like || long_number || long_hex
}

fn validate_repo_path(path: &str) -> Result<(), IntelligenceError> {
    if path.is_empty()
        || path.len() > 1024
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.contains("..")
        || path.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err(IntelligenceError::InvalidRepositoryPath);
    }
    Ok(())
}

fn validate_identifier(value: &str, max_len: usize) -> Result<(), IntelligenceError> {
    if value.is_empty()
        || value.len() > max_len
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(IntelligenceError::InvalidIdentifier);
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum IntelligenceError {
    #[error("too many changed files")]
    TooManyChangedFiles,
    #[error("invalid repository path")]
    InvalidRepositoryPath,
    #[error("too much historical failure data")]
    TooMuchHistory,
    #[error("invalid historical failure record")]
    InvalidFailureRecord,
    #[error("too many workers")]
    TooManyWorkers,
    #[error("invalid worker capacity")]
    InvalidWorkerCapacity,
    #[error("too many scheduling tasks")]
    TooManyTasks,
    #[error("invalid identifier")]
    InvalidIdentifier,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_change_selects_gateway_profile() {
        let changes = ChangeSet {
            files: vec!["crates/df-test-mcp/src/lib.rs".into()],
        };
        let recommendations = recommend_profiles(&changes).unwrap();
        let mcp = recommendations
            .iter()
            .find(|item| item.profile == TestProfile::McpGateway)
            .unwrap();
        assert!(mcp.score >= 65);
    }

    #[test]
    fn cross_crate_changes_trigger_full_regression() {
        let changes = ChangeSet {
            files: vec![
                "crates/a/src/lib.rs".into(),
                "crates/b/src/lib.rs".into(),
                "crates/c/src/lib.rs".into(),
                "crates/d/src/lib.rs".into(),
            ],
        };
        let recommendations = recommend_profiles(&changes).unwrap();
        assert!(recommendations
            .iter()
            .any(|item| item.profile == TestProfile::FullRegression && item.score >= 60));
    }

    #[test]
    fn historical_overlap_increases_target_score() {
        let changes = ChangeSet {
            files: vec!["crates/df-test-gui/src/lib.rs".into()],
        };
        let history = vec![HistoricalFailure {
            id: Uuid::new_v4(),
            profile: TestProfile::GuiAutomation,
            step: "gui fixture".into(),
            message: "assertion failed".into(),
            changed_files: vec!["crates/df-test-gui/src/lib.rs".into()],
            unix_time_secs: 100,
        }];
        let input = IntelligenceInput {
            changes,
            history,
            workers: Vec::new(),
        };
        let report = analyze(&input).unwrap();
        let gui = report
            .recommendations
            .iter()
            .find(|item| item.profile == TestProfile::GuiAutomation)
            .unwrap();
        assert!(gui.score >= 77);
    }

    #[test]
    fn similar_failures_cluster_despite_volatile_paths_and_numbers() {
        let history = vec![
            HistoricalFailure {
                id: Uuid::new_v4(),
                profile: TestProfile::RustStandard,
                step: "cargo test".into(),
                message: r#"failed at C:\tmp\build\12345\foo.rs line 8123"#.into(),
                changed_files: vec!["crates/a/src/lib.rs".into()],
                unix_time_secs: 10,
            },
            HistoricalFailure {
                id: Uuid::new_v4(),
                profile: TestProfile::RustStandard,
                step: "cargo test".into(),
                message: r#"failed at C:\tmp\build\99999\foo.rs line 9123"#.into(),
                changed_files: vec!["crates/a/src/lib.rs".into()],
                unix_time_secs: 20,
            },
        ];
        let clusters = cluster_failures(&history).unwrap();
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].occurrences, 2);
    }

    #[test]
    fn scheduler_prefers_lower_load_eligible_worker() {
        let recommendations = vec![ProfileRecommendation {
            profile: TestProfile::RustStandard,
            score: 80,
            reasons: vec!["fixture".into()],
        }];
        let workers = vec![
            WorkerCapacity {
                worker_id: "worker-a".into(),
                supported_profiles: [TestProfile::RustStandard].into_iter().collect(),
                total_memory_mib: 8192,
                free_memory_mib: 4096,
                max_parallel_jobs: 2,
                active_jobs: 0,
                load_percent: 70,
            },
            WorkerCapacity {
                worker_id: "worker-b".into(),
                supported_profiles: [TestProfile::RustStandard].into_iter().collect(),
                total_memory_mib: 8192,
                free_memory_mib: 4096,
                max_parallel_jobs: 2,
                active_jobs: 0,
                load_percent: 10,
            },
        ];
        let (scheduled, unscheduled) = schedule_profiles(&recommendations, &workers).unwrap();
        assert_eq!(scheduled.len(), 1);
        assert_eq!(scheduled[0].worker_id, "worker-b");
        assert!(unscheduled.is_empty());
    }

    #[test]
    fn scheduler_respects_memory_and_slots() {
        let recommendations = vec![ProfileRecommendation {
            profile: TestProfile::RustDeep,
            score: 90,
            reasons: vec!["fixture".into()],
        }];
        let workers = vec![WorkerCapacity {
            worker_id: "worker-a".into(),
            supported_profiles: [TestProfile::RustDeep].into_iter().collect(),
            total_memory_mib: 4096,
            free_memory_mib: 2048,
            max_parallel_jobs: 1,
            active_jobs: 1,
            load_percent: 20,
        }];
        let (scheduled, unscheduled) = schedule_profiles(&recommendations, &workers).unwrap();
        assert!(scheduled.is_empty());
        assert_eq!(unscheduled, vec![TestProfile::RustDeep]);
    }
}
