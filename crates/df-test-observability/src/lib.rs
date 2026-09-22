use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;

pub const MAX_LOG_MESSAGE_BYTES: usize = 8 * 1024;
pub const MAX_LOG_FIELDS: usize = 32;
pub const MAX_LOG_FIELD_BYTES: usize = 16 * 1024;
pub const MAX_METRIC_NAME_BYTES: usize = 128;
pub const MAX_METRIC_LABELS: usize = 16;
pub const MAX_ARTIFACTS_PER_PRUNE: usize = 10_000;
pub const MAX_LOG_ROTATIONS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuredLogEvent {
    pub unix_time_secs: u64,
    pub level: LogLevel,
    pub component: String,
    pub message: String,
    #[serde(default)]
    pub fields: BTreeMap<String, serde_json::Value>,
    pub job_id: Option<String>,
    pub worker_id: Option<String>,
}

impl StructuredLogEvent {
    pub fn validate(&self) -> Result<(), ObservabilityError> {
        validate_identifier(&self.component, 128)?;
        if self.message.is_empty() || self.message.len() > MAX_LOG_MESSAGE_BYTES {
            return Err(ObservabilityError::InvalidLogMessage);
        }
        if self.fields.len() > MAX_LOG_FIELDS {
            return Err(ObservabilityError::TooManyLogFields);
        }
        for (key, value) in &self.fields {
            validate_identifier(key, 128)?;
            if serde_json::to_vec(value)?.len() > MAX_LOG_FIELD_BYTES {
                return Err(ObservabilityError::LogFieldTooLarge);
            }
        }
        if let Some(job_id) = &self.job_id {
            validate_reference(job_id)?;
        }
        if let Some(worker_id) = &self.worker_id {
            validate_reference(worker_id)?;
        }
        Ok(())
    }

    pub fn redacted(mut self) -> Self {
        for (key, value) in &mut self.fields {
            if is_sensitive_key(key) {
                *value = serde_json::Value::String("<redacted>".to_owned());
            }
        }
        self
    }
}

pub struct JsonlLogWriter {
    path: PathBuf,
    max_bytes: u64,
}

impl JsonlLogWriter {
    pub fn new(path: impl Into<PathBuf>, max_bytes: u64) -> Result<Self, ObservabilityError> {
        let path = path.into();
        if max_bytes < 1024 || max_bytes > 1024 * 1024 * 1024 {
            return Err(ObservabilityError::InvalidLogLimit);
        }
        validate_leaf_file_path(&path)?;
        Ok(Self { path, max_bytes })
    }

    pub fn append(&self, event: StructuredLogEvent) -> Result<(), ObservabilityError> {
        event.validate()?;
        let event = event.redacted();
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let line = serde_json::to_vec(&event)?;
        let projected = self
            .path
            .metadata()
            .map(|metadata| metadata.len())
            .unwrap_or(0)
            .saturating_add(u64::try_from(line.len() + 1).unwrap_or(u64::MAX));
        if projected > self.max_bytes {
            self.rotate()?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(&line)?;
        file.write_all(b"\n")?;
        file.flush()?;
        Ok(())
    }

    fn rotate(&self) -> Result<(), ObservabilityError> {
        for index in (1..MAX_LOG_ROTATIONS).rev() {
            let source = rotated_path(&self.path, index);
            let destination = rotated_path(&self.path, index + 1);
            if source.exists() {
                if destination.exists() {
                    fs::remove_file(&destination)?;
                }
                fs::rename(source, destination)?;
            }
        }

        if self.path.exists() {
            let first = rotated_path(&self.path, 1);
            if first.exists() {
                fs::remove_file(&first)?;
            }
            fs::rename(&self.path, first)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricPoint {
    pub unix_time_secs: u64,
    pub name: String,
    pub value: f64,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}

impl MetricPoint {
    pub fn validate(&self) -> Result<(), ObservabilityError> {
        validate_metric_name(&self.name)?;
        if !self.value.is_finite() {
            return Err(ObservabilityError::InvalidMetricValue);
        }
        if self.labels.len() > MAX_METRIC_LABELS {
            return Err(ObservabilityError::TooManyMetricLabels);
        }
        for (key, value) in &self.labels {
            validate_identifier(key, 64)?;
            validate_reference(value)?;
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricsRegistry {
    counters: BTreeMap<String, u64>,
    gauges: BTreeMap<String, f64>,
}

impl MetricsRegistry {
    pub fn increment(&mut self, name: &str, amount: u64) -> Result<(), ObservabilityError> {
        validate_metric_name(name)?;
        let entry = self.counters.entry(name.to_owned()).or_insert(0);
        *entry = entry.saturating_add(amount);
        Ok(())
    }

    pub fn set_gauge(&mut self, name: &str, value: f64) -> Result<(), ObservabilityError> {
        validate_metric_name(name)?;
        if !value.is_finite() {
            return Err(ObservabilityError::InvalidMetricValue);
        }
        self.gauges.insert(name.to_owned(), value);
        Ok(())
    }

    pub fn snapshot(&self, unix_time_secs: u64) -> Vec<MetricPoint> {
        let counters = self.counters.iter().map(|(name, value)| MetricPoint {
            unix_time_secs,
            name: name.clone(),
            value: *value as f64,
            labels: BTreeMap::new(),
        });
        let gauges = self.gauges.iter().map(|(name, value)| MetricPoint {
            unix_time_secs,
            name: name.clone(),
            value: *value,
            labels: BTreeMap::new(),
        });
        counters.chain(gauges).collect()
    }

    pub fn render_text(&self) -> String {
        let mut output = String::new();
        for (name, value) in &self.counters {
            output.push_str(name);
            output.push(' ');
            output.push_str(&value.to_string());
            output.push('\n');
        }
        for (name, value) in &self.gauges {
            output.push_str(name);
            output.push(' ');
            output.push_str(&value.to_string());
            output.push('\n');
        }
        output
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactCatalogEntry {
    pub relative_path: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub created_at_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRetentionPolicy {
    pub max_age_secs: u64,
    pub max_total_bytes: u64,
    pub max_artifacts: usize,
}

impl ArtifactRetentionPolicy {
    pub fn validate(&self) -> Result<(), ObservabilityError> {
        if self.max_age_secs == 0
            || self.max_total_bytes == 0
            || self.max_artifacts == 0
            || self.max_artifacts > MAX_ARTIFACTS_PER_PRUNE
        {
            return Err(ObservabilityError::InvalidRetentionPolicy);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactPruneReport {
    pub removed: Vec<String>,
    pub retained_count: usize,
    pub retained_bytes: u64,
}

pub fn catalog_artifact(
    artifact_root: impl AsRef<Path>,
    artifact_path: impl AsRef<Path>,
    created_at_secs: u64,
) -> Result<ArtifactCatalogEntry, ObservabilityError> {
    let root = canonical_directory(artifact_root.as_ref())?;
    let path = fs::canonicalize(artifact_path.as_ref())?;
    if !path.starts_with(&root) {
        return Err(ObservabilityError::ArtifactOutsideRoot);
    }
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(ObservabilityError::InvalidArtifactFile);
    }

    let relative = path
        .strip_prefix(&root)
        .map_err(|_| ObservabilityError::ArtifactOutsideRoot)?;
    let relative_path = normalize_relative(relative)?;
    let sha256 = hash_file(&path)?;
    Ok(ArtifactCatalogEntry {
        relative_path,
        size_bytes: metadata.len(),
        sha256,
        created_at_secs,
    })
}

pub fn prune_artifacts(
    artifact_root: impl AsRef<Path>,
    entries: &[ArtifactCatalogEntry],
    policy: ArtifactRetentionPolicy,
    now_secs: u64,
) -> Result<ArtifactPruneReport, ObservabilityError> {
    policy.validate()?;
    if entries.len() > MAX_ARTIFACTS_PER_PRUNE {
        return Err(ObservabilityError::TooManyArtifacts);
    }
    let root = canonical_directory(artifact_root.as_ref())?;

    let mut ordered = entries.to_vec();
    ordered.sort_by_key(|entry| (entry.created_at_secs, entry.relative_path.clone()));

    let mut retained: BTreeSet<String> =
        ordered.iter().map(|entry| entry.relative_path.clone()).collect();
    let mut retained_bytes = ordered
        .iter()
        .fold(0u64, |total, entry| total.saturating_add(entry.size_bytes));

    for entry in &ordered {
        let expired = now_secs.saturating_sub(entry.created_at_secs) > policy.max_age_secs;
        let over_count = retained.len() > policy.max_artifacts;
        let over_bytes = retained_bytes > policy.max_total_bytes;
        if !expired && !over_count && !over_bytes {
            continue;
        }

        let relative = safe_relative_path(&entry.relative_path)?;
        let candidate = root.join(relative);
        if candidate.exists() {
            let metadata = fs::symlink_metadata(&candidate)?;
            if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
                return Err(ObservabilityError::InvalidArtifactFile);
            }
            let canonical = fs::canonicalize(&candidate)?;
            if !canonical.starts_with(&root) {
                return Err(ObservabilityError::ArtifactOutsideRoot);
            }
            fs::remove_file(canonical)?;
        }

        if retained.remove(&entry.relative_path) {
            retained_bytes = retained_bytes.saturating_sub(entry.size_bytes);
        }
    }

    let removed = ordered
        .iter()
        .filter(|entry| !retained.contains(&entry.relative_path))
        .map(|entry| entry.relative_path.clone())
        .collect();

    Ok(ArtifactPruneReport {
        removed,
        retained_count: retained.len(),
        retained_bytes,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditDigest {
    pub sequence: u64,
    pub previous_sha256: String,
    pub event_sha256: String,
}

pub fn next_audit_digest(
    sequence: u64,
    previous_sha256: Option<&str>,
    event: &serde_json::Value,
) -> Result<AuditDigest, ObservabilityError> {
    let previous = previous_sha256.unwrap_or(&"0".repeat(64)).to_owned();
    validate_sha256(&previous)?;
    let payload = serde_json::to_vec(event)?;
    let mut hasher = Sha256::new();
    hasher.update(sequence.to_be_bytes());
    hasher.update(previous.as_bytes());
    hasher.update(&payload);
    Ok(AuditDigest {
        sequence,
        previous_sha256: previous,
        event_sha256: hex::encode(hasher.finalize()),
    })
}

fn hash_file(path: &Path) -> Result<String, ObservabilityError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn canonical_directory(path: &Path) -> Result<PathBuf, ObservabilityError> {
    fs::create_dir_all(path)?;
    let canonical = fs::canonicalize(path)?;
    if !fs::metadata(&canonical)?.is_dir() {
        return Err(ObservabilityError::InvalidArtifactRoot);
    }
    Ok(canonical)
}

fn safe_relative_path(value: &str) -> Result<PathBuf, ObservabilityError> {
    if value.is_empty() || value.len() > 2048 || value.contains('\0') {
        return Err(ObservabilityError::InvalidRelativePath);
    }
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(ObservabilityError::InvalidRelativePath);
    }
    Ok(path.to_path_buf())
}

fn normalize_relative(path: &Path) -> Result<String, ObservabilityError> {
    let value = path.to_string_lossy().replace('\\', "/");
    safe_relative_path(&value)?;
    Ok(value)
}

fn rotated_path(path: &Path, index: usize) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(format!(".{index}"));
    PathBuf::from(value)
}

fn validate_leaf_file_path(path: &Path) -> Result<(), ObservabilityError> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(ObservabilityError::InvalidLogPath)?;
    if name.is_empty() || name.len() > 255 || matches!(name, "." | "..") {
        return Err(ObservabilityError::InvalidLogPath);
    }
    Ok(())
}

fn validate_metric_name(value: &str) -> Result<(), ObservabilityError> {
    if value.is_empty()
        || value.len() > MAX_METRIC_NAME_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b':' | b'.'))
    {
        return Err(ObservabilityError::InvalidMetricName);
    }
    Ok(())
}

fn validate_identifier(value: &str, max: usize) -> Result<(), ObservabilityError> {
    if value.is_empty()
        || value.len() > max
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(ObservabilityError::InvalidIdentifier);
    }
    Ok(())
}

fn validate_reference(value: &str) -> Result<(), ObservabilityError> {
    if value.is_empty()
        || value.len() > 256
        || value.chars().any(|ch| ch == '\r' || ch == '\n' || ch == '\0')
    {
        return Err(ObservabilityError::InvalidReference);
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), ObservabilityError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ObservabilityError::InvalidSha256);
    }
    Ok(())
}

fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    ["password", "passwd", "token", "secret", "private_key", "authorization"]
        .iter()
        .any(|needle| lower.contains(needle))
}

#[derive(Debug, Error)]
pub enum ObservabilityError {
    #[error("invalid log message")]
    InvalidLogMessage,
    #[error("too many structured log fields")]
    TooManyLogFields,
    #[error("structured log field is too large")]
    LogFieldTooLarge,
    #[error("invalid log file size limit")]
    InvalidLogLimit,
    #[error("invalid log path")]
    InvalidLogPath,
    #[error("invalid identifier")]
    InvalidIdentifier,
    #[error("invalid reference")]
    InvalidReference,
    #[error("invalid metric name")]
    InvalidMetricName,
    #[error("invalid metric value")]
    InvalidMetricValue,
    #[error("too many metric labels")]
    TooManyMetricLabels,
    #[error("invalid artifact root")]
    InvalidArtifactRoot,
    #[error("artifact is outside the configured root")]
    ArtifactOutsideRoot,
    #[error("artifact must be a regular non-symlink file")]
    InvalidArtifactFile,
    #[error("invalid artifact relative path")]
    InvalidRelativePath,
    #[error("invalid artifact retention policy")]
    InvalidRetentionPolicy,
    #[error("too many artifacts in a single retention pass")]
    TooManyArtifacts,
    #[error("invalid SHA-256 value")]
    InvalidSha256,
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_log_redacts_secret_fields() {
        let root = std::env::temp_dir().join(format!(
            "dragonforge-phase14-log-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("worker.jsonl");
        let writer = JsonlLogWriter::new(&path, 4096).unwrap();
        let mut fields = BTreeMap::new();
        fields.insert(
            "api_token".into(),
            serde_json::Value::String("do-not-write-me".into()),
        );
        writer
            .append(StructuredLogEvent {
                unix_time_secs: 10,
                level: LogLevel::Info,
                component: "worker".into(),
                message: "connected".into(),
                fields,
                job_id: None,
                worker_id: Some("worker-01".into()),
            })
            .unwrap();
        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("<redacted>"));
        assert!(!content.contains("do-not-write-me"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn metric_registry_is_bounded_and_renderable() {
        let mut registry = MetricsRegistry::default();
        registry.increment("jobs_total", 2).unwrap();
        registry.set_gauge("workers_online", 3.0).unwrap();
        let text = registry.render_text();
        assert!(text.contains("jobs_total 2"));
        assert!(text.contains("workers_online 3"));
        assert_eq!(registry.snapshot(100).len(), 2);
    }

    #[test]
    fn artifact_catalog_hashes_and_prunes_inside_root() {
        let root = std::env::temp_dir().join(format!(
            "dragonforge-phase14-artifacts-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let old = root.join("old.log");
        let new = root.join("new.log");
        fs::write(&old, b"old").unwrap();
        fs::write(&new, b"new-data").unwrap();
        let entries = vec![
            catalog_artifact(&root, &old, 10).unwrap(),
            catalog_artifact(&root, &new, 90).unwrap(),
        ];
        let report = prune_artifacts(
            &root,
            &entries,
            ArtifactRetentionPolicy {
                max_age_secs: 50,
                max_total_bytes: 1024,
                max_artifacts: 10,
            },
            100,
        )
        .unwrap();
        assert_eq!(report.removed, vec!["old.log"]);
        assert!(!old.exists());
        assert!(new.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn artifact_prune_rejects_parent_traversal() {
        let root = std::env::temp_dir().join(format!(
            "dragonforge-phase14-traversal-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let result = prune_artifacts(
            &root,
            &[ArtifactCatalogEntry {
                relative_path: "../outside.txt".into(),
                size_bytes: 1,
                sha256: "0".repeat(64),
                created_at_secs: 1,
            }],
            ArtifactRetentionPolicy {
                max_age_secs: 1,
                max_total_bytes: 1,
                max_artifacts: 1,
            },
            100,
        );
        assert!(matches!(result, Err(ObservabilityError::InvalidRelativePath)));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn audit_digest_chains_previous_hash() {
        let first = next_audit_digest(1, None, &serde_json::json!({"kind":"start"})).unwrap();
        let second = next_audit_digest(
            2,
            Some(&first.event_sha256),
            &serde_json::json!({"kind":"finish"}),
        )
        .unwrap();
        assert_eq!(second.previous_sha256, first.event_sha256);
        assert_ne!(first.event_sha256, second.event_sha256);
    }
}
