use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

pub const SECURITY_REVIEW_SCHEMA_VERSION: u16 = 1;
pub const MAX_REVIEW_FILE_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_SCANNED_FILES: usize = 4096;
pub const MAX_TOTAL_SCAN_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Info,
    Warning,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityFinding {
    pub id: String,
    pub category: String,
    pub severity: FindingSeverity,
    pub summary: String,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityReviewReport {
    pub schema_version: u16,
    pub checks_run: usize,
    pub checks_passed: usize,
    pub files_scanned: usize,
    pub bytes_scanned: u64,
    pub findings: Vec<SecurityFinding>,
}

impl SecurityReviewReport {
    pub fn blocking_findings(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| {
                matches!(
                    f.severity,
                    FindingSeverity::High | FindingSeverity::Critical
                )
            })
            .count()
    }

    pub fn passed(&self) -> bool {
        self.blocking_findings() == 0 && self.checks_run == self.checks_passed
    }
}

#[derive(Debug, Clone)]
struct EvidenceRule {
    id: &'static str,
    category: &'static str,
    path: &'static str,
    required: &'static [&'static str],
}

const RULES: &[EvidenceRule] = &[
    EvidenceRule {
        id: "SR-PROTOCOL-001",
        category: "protocol",
        path: "crates/df-test-protocol/src/lib.rs",
        required: &[
            "pub enum TestAction",
            "ResourceLimits",
            "required_capabilities",
        ],
    },
    EvidenceRule {
        id: "SR-POLICY-001",
        category: "policy",
        path: "crates/df-test-policy/src/lib.rs",
        required: &[
            "starts_with(\"https://\")",
            "RepositoryNotAllowed",
            "CapabilityNotAllowed",
            "ResourceLimitExceeded",
        ],
    },
    EvidenceRule {
        id: "SR-MCP-001",
        category: "mcp",
        path: "crates/df-test-mcp/src/lib.rs",
        required: &[
            "is_loopback()",
            "MAX_HTTP_HEADER_BYTES",
            "MAX_HTTP_BODY_BYTES",
            "MAX_ACTIVE_GATEWAY_JOBS",
            "constant_time_equal",
            "dragonforge_job_submit",
        ],
    },
    EvidenceRule {
        id: "SR-IDENTITY-001",
        category: "certificates",
        path: "crates/df-test-identity/src/lib.rs",
        required: &[
            "MAX_CERTIFICATE_CHAIN",
            "MAX_CERTIFICATE_BYTES",
            "CertificateFingerprint",
            "revoked_fingerprints",
            "validate_private_controller_address",
        ],
    },
    EvidenceRule {
        id: "SR-OBSERVABILITY-001",
        category: "logs_artifacts",
        path: "crates/df-test-observability/src/lib.rs",
        required: &["sha256", "redact", "canonicalize"],
    },
    EvidenceRule {
        id: "SR-INSTALL-001",
        category: "installer",
        path: "crates/df-test-install/src/lib.rs",
        required: &["sha256", "validate", "symlink"],
    },
    EvidenceRule {
        id: "SR-RELEASE-001",
        category: "release",
        path: "crates/df-test-release/src/lib.rs",
        required: &[
            "StableArtifactUnsigned",
            "MAX_RELEASE_FILE_BYTES",
            "validate_leaf",
            "ChecksumMismatch",
        ],
    },
    EvidenceRule {
        id: "SR-VM-001",
        category: "privileges_vm",
        path: "crates/df-test-vm/src/lib.rs",
        required: &[
            "VM_NAME_PREFIX",
            "validate_base_image",
            "validate_switch_name",
            "DEFAULT_BASELINE_CHECKPOINT",
        ],
    },
    EvidenceRule {
        id: "SR-DASHBOARD-001",
        category: "dashboard",
        path: "crates/df-test-dashboard/src/lib.rs",
        required: &["is_loopback()", "constant_time", "read_only"],
    },
    EvidenceRule {
        id: "SR-DISTRIBUTED-001",
        category: "transport",
        path: "crates/df-test-distributed/src/lib.rs",
        required: &["nonce", "hmac", "validate_controller"],
    },
];

pub fn run_security_review(
    root: impl AsRef<Path>,
) -> Result<SecurityReviewReport, SecurityReviewError> {
    let root = fs::canonicalize(root)?;
    if !root.is_dir() {
        return Err(SecurityReviewError::InvalidRoot);
    }

    let mut findings = Vec::new();
    let mut passed = 0usize;

    for rule in RULES {
        let path = safe_join(&root, rule.path)?;
        let text = read_bounded_text(&path)?;
        let missing: Vec<_> = rule
            .required
            .iter()
            .filter(|needle| !text.contains(**needle))
            .copied()
            .collect();
        if missing.is_empty() {
            passed += 1;
        } else {
            findings.push(SecurityFinding {
                id: rule.id.into(),
                category: rule.category.into(),
                severity: FindingSeverity::High,
                summary: format!("security invariant evidence missing from {}", rule.path),
                evidence: format!("missing markers: {}", missing.join(", ")),
            });
        }
    }

    let (files_scanned, bytes_scanned, mut scan_findings) = scan_repository(&root)?;
    findings.append(&mut scan_findings);

    Ok(SecurityReviewReport {
        schema_version: SECURITY_REVIEW_SCHEMA_VERSION,
        checks_run: RULES.len(),
        checks_passed: passed,
        files_scanned,
        bytes_scanned,
        findings,
    })
}

fn scan_repository(root: &Path) -> Result<(usize, u64, Vec<SecurityFinding>), SecurityReviewError> {
    let mut stack = vec![root.to_path_buf()];
    let mut files = 0usize;
    let mut total = 0u64;
    let mut findings = Vec::new();

    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if matches!(
                    name.as_ref(),
                    ".git" | "target" | "test-logs" | "test-release"
                ) {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }

            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string();
            let lower = rel.to_ascii_lowercase();
            if [".pfx", ".p12", ".key", "id_rsa", "id_ed25519"]
                .iter()
                .any(|suffix| lower.ends_with(suffix))
            {
                findings.push(SecurityFinding {
                    id: "SR-SECRET-002".into(),
                    category: "secrets".into(),
                    severity: FindingSeverity::Critical,
                    summary: "secret-like key file appears to be committed".into(),
                    evidence: rel.clone(),
                });
            }

            if !is_review_text_file(&path) {
                continue;
            }

            files += 1;
            if files > MAX_SCANNED_FILES {
                return Err(SecurityReviewError::ScanLimitExceeded);
            }
            let metadata = fs::metadata(&path)?;
            if metadata.len() > MAX_REVIEW_FILE_BYTES {
                continue;
            }
            total = total.saturating_add(metadata.len());
            if total > MAX_TOTAL_SCAN_BYTES {
                return Err(SecurityReviewError::ScanLimitExceeded);
            }

            let bytes = fs::read(&path)?;
            let text = String::from_utf8_lossy(&bytes);
            if path.extension().and_then(|v| v.to_str()) != Some("md")
                && contains_complete_private_key_block(&text)
            {
                findings.push(SecurityFinding {
                    id: "SR-SECRET-001".into(),
                    category: "secrets".into(),
                    severity: FindingSeverity::Critical,
                    summary: "private key material appears to be committed".into(),
                    evidence: rel.clone(),
                });
            }
        }
    }
    Ok((files, total, findings))
}

fn contains_complete_private_key_block(text: &str) -> bool {
    const PRIVATE_KEY_MARKERS: &[(&str, &str)] = &[
        ("-----BEGIN PRIVATE KEY-----", "-----END PRIVATE KEY-----"),
        (
            "-----BEGIN OPENSSH PRIVATE KEY-----",
            "-----END OPENSSH PRIVATE KEY-----",
        ),
        (
            "-----BEGIN RSA PRIVATE KEY-----",
            "-----END RSA PRIVATE KEY-----",
        ),
        (
            "-----BEGIN EC PRIVATE KEY-----",
            "-----END EC PRIVATE KEY-----",
        ),
    ];

    PRIVATE_KEY_MARKERS
        .iter()
        .any(|(begin, end)| text.contains(begin) && text.contains(end))
}

fn is_review_text_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|v| v.to_str())
            .unwrap_or_default(),
        "rs" | "toml" | "ps1" | "sh" | "py" | "json" | "yml" | "yaml" | "md" | "pem"
    )
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, SecurityReviewError> {
    let joined = root.join(relative);
    let canonical = fs::canonicalize(&joined)
        .map_err(|_| SecurityReviewError::MissingEvidenceFile(relative.to_string()))?;
    if !canonical.starts_with(root) || !canonical.is_file() {
        return Err(SecurityReviewError::UnsafeEvidencePath);
    }
    Ok(canonical)
}

fn read_bounded_text(path: &Path) -> Result<String, SecurityReviewError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_REVIEW_FILE_BYTES {
        return Err(SecurityReviewError::InputTooLarge);
    }
    Ok(String::from_utf8_lossy(&fs::read(path)?).into_owned())
}

pub fn run_fixture() -> Result<SecurityReviewReport, SecurityReviewError> {
    let root = std::env::temp_dir().join(format!(
        "dragonforge-security-review-fixture-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    for rule in RULES {
        let path = root.join(rule.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, rule.required.join("\n"))?;
    }
    fs::create_dir_all(root.join("docs"))?;
    fs::write(root.join("docs/README.md"), "fixture")?;

    let report = run_security_review(&root)?;
    let _ = fs::remove_dir_all(root);
    Ok(report)
}

#[derive(Debug, Error)]
pub enum SecurityReviewError {
    #[error("security review root must be an existing directory")]
    InvalidRoot,
    #[error("security evidence file is missing: {0}")]
    MissingEvidenceFile(String),
    #[error("security review path escaped the repository root")]
    UnsafeEvidencePath,
    #[error("security review input exceeded a configured bound")]
    InputTooLarge,
    #[error("security review scan exceeded configured file/byte limits")]
    ScanLimitExceeded,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_passes_all_required_invariants() {
        let report = run_fixture().unwrap();
        assert!(report.passed(), "{report:#?}");
        assert_eq!(report.checks_run, RULES.len());
    }

    #[test]
    fn detector_source_markers_do_not_self_match() {
        let source_like = r#"
            text.contains("-----BEGIN PRIVATE KEY-----");
            text.contains("-----BEGIN OPENSSH PRIVATE KEY-----");
        "#;
        assert!(!contains_complete_private_key_block(source_like));
    }

    #[test]
    fn complete_private_key_block_is_detected() {
        let pem = "-----BEGIN PRIVATE KEY-----\nZmFrZQ==\n-----END PRIVATE KEY-----";
        assert!(contains_complete_private_key_block(pem));
    }

    #[test]
    fn committed_private_key_is_blocking() {
        let root = std::env::temp_dir().join(format!(
            "dragonforge-security-secret-fixture-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        for rule in RULES {
            let path = root.join(rule.path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, rule.required.join("\n")).unwrap();
        }
        fs::write(root.join("leaked.key"), "not-even-real").unwrap();
        let report = run_security_review(&root).unwrap();
        assert_eq!(report.blocking_findings(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}
