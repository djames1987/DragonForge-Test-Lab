use serde::Deserialize;
use std::process::{Command, Output};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubRepository {
    owner: String,
    name: String,
}

impl GitHubRepository {
    pub fn parse_https(url: &str) -> Result<Self, GitHubError> {
        let prefix = "https://github.com/";
        let rest = url
            .strip_prefix(prefix)
            .ok_or(GitHubError::UnsupportedRepositoryUrl)?;
        let rest = rest.strip_suffix(".git").unwrap_or(rest);
        let mut parts = rest.split('/');

        let owner = parts.next().unwrap_or_default();
        let name = parts.next().unwrap_or_default();

        if parts.next().is_some()
            || !valid_repository_component(owner)
            || !valid_repository_component(name)
        {
            return Err(GitHubError::UnsupportedRepositoryUrl);
        }

        Ok(Self {
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }

    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitStatusState {
    Pending,
    Success,
    Failure,
    Error,
}

impl CommitStatusState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitStatus {
    state: CommitStatusState,
    context: String,
    description: String,
}

impl CommitStatus {
    pub fn new(
        state: CommitStatusState,
        context: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<Self, GitHubError> {
        let context = context.into();
        let description = description.into();

        if context.is_empty() || context.len() > 100 || description.len() > 140 {
            return Err(GitHubError::InvalidStatusMetadata);
        }

        Ok(Self {
            state,
            context,
            description,
        })
    }
}

#[derive(Debug, Clone)]
pub struct GhGitHubClient {
    program: String,
}

impl Default for GhGitHubClient {
    fn default() -> Self {
        Self {
            program: "gh".into(),
        }
    }
}

impl GhGitHubClient {
    pub fn doctor(&self) -> Result<String, GitHubError> {
        let output = self.run(&auth_status_args())?;
        let text = combined_output(&output);
        if output.status.success() {
            Ok(text)
        } else {
            Err(GitHubError::CommandFailed {
                operation: "gh auth status",
                output: text,
            })
        }
    }

    pub fn resolve_commit(
        &self,
        repository: &GitHubRepository,
        revision: &str,
    ) -> Result<String, GitHubError> {
        validate_revision(revision)?;
        let output = self.run(&resolve_commit_args(repository, revision))?;
        let text = combined_output(&output);

        if !output.status.success() {
            return Err(GitHubError::CommandFailed {
                operation: "resolve commit",
                output: text,
            });
        }

        let payload: CommitPayload = serde_json::from_slice(&output.stdout)?;
        validate_commit_sha(&payload.sha)?;
        Ok(payload.sha)
    }

    pub fn set_commit_status(
        &self,
        repository: &GitHubRepository,
        sha: &str,
        status: &CommitStatus,
    ) -> Result<(), GitHubError> {
        validate_commit_sha(sha)?;
        let args = commit_status_args(repository, sha, status);
        let output = self.run(&args)?;

        if output.status.success() {
            Ok(())
        } else {
            Err(GitHubError::CommandFailed {
                operation: "set commit status",
                output: combined_output(&output),
            })
        }
    }

    fn run(&self, args: &[String]) -> Result<Output, GitHubError> {
        Command::new(&self.program)
            .args(args)
            .output()
            .map_err(|source| GitHubError::Spawn {
                program: self.program.clone(),
                source,
            })
    }
}

#[derive(Debug, Deserialize)]
struct CommitPayload {
    sha: String,
}

fn auth_status_args() -> Vec<String> {
    vec![
        "auth".into(),
        "status".into(),
        "--hostname".into(),
        "github.com".into(),
    ]
}

fn resolve_commit_args(repository: &GitHubRepository, revision: &str) -> Vec<String> {
    vec![
        "api".into(),
        format!(
            "repos/{}/{}/commits/{}",
            repository.owner,
            repository.name,
            encode_path_segment(revision)
        ),
    ]
}

fn commit_status_args(
    repository: &GitHubRepository,
    sha: &str,
    status: &CommitStatus,
) -> Vec<String> {
    let args = vec![
        "api".into(),
        format!(
            "repos/{}/{}/statuses/{}",
            repository.owner, repository.name, sha
        ),
        "--method".into(),
        "POST".into(),
        "-f".into(),
        format!("state={}", status.state.as_str()),
        "-f".into(),
        format!("context={}", status.context),
        "-f".into(),
        format!("description={}", status.description),
    ];

    args
}

fn validate_revision(revision: &str) -> Result<(), GitHubError> {
    let valid = !revision.is_empty()
        && revision.len() <= 256
        && !revision.starts_with('-')
        && revision
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'));

    if valid {
        Ok(())
    } else {
        Err(GitHubError::InvalidRevision)
    }
}

fn validate_commit_sha(sha: &str) -> Result<(), GitHubError> {
    if sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(GitHubError::InvalidCommitSha)
    }
}

fn valid_repository_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.len() <= 100
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());

    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }

    encoded
}

fn combined_output(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    format!("{}{}", stdout, stderr).trim().to_owned()
}

#[derive(Debug, Error)]
pub enum GitHubError {
    #[error("repository URL must be https://github.com/<owner>/<repository>[.git]")]
    UnsupportedRepositoryUrl,
    #[error("revision contains unsupported characters")]
    InvalidRevision,
    #[error("GitHub returned an invalid commit SHA")]
    InvalidCommitSha,
    #[error("commit status context/description is invalid")]
    InvalidStatusMetadata,
    #[error("failed to spawn {program}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{operation} failed: {output}")]
    CommandFailed {
        operation: &'static str,
        output: String,
    },
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_github_https_urls() {
        let repo = GitHubRepository::parse_https(
            "https://github.com/djames1987/DragonForge-Test-Lab.git",
        )
        .unwrap();
        assert_eq!(repo.owner, "djames1987");
        assert_eq!(repo.name, "DragonForge-Test-Lab");
        assert_eq!(repo.slug(), "djames1987/DragonForge-Test-Lab");
    }

    #[test]
    fn rejects_non_github_or_nested_urls() {
        assert!(GitHubRepository::parse_https("https://example.com/a/b.git").is_err());
        assert!(GitHubRepository::parse_https("https://github.com/a/b/extra").is_err());
    }

    #[test]
    fn encodes_branch_slashes_for_commit_endpoint() {
        let repo = GitHubRepository {
            owner: "owner".into(),
            name: "repo".into(),
        };
        let args = resolve_commit_args(&repo, "feature/test-1");
        assert_eq!(args, vec!["api", "repos/owner/repo/commits/feature%2Ftest-1"]);
    }

    #[test]
    fn status_arguments_are_fixed_and_typed() {
        let repo = GitHubRepository {
            owner: "owner".into(),
            name: "repo".into(),
        };
        let status = CommitStatus::new(
            CommitStatusState::Pending,
            "dragonforge/test-lab",
            "DragonForge Test Lab is running",
        )
        .unwrap();

        let args = commit_status_args(
            &repo,
            "0123456789abcdef0123456789abcdef01234567",
            &status,
        );

        assert_eq!(args[0], "api");
        assert_eq!(
            args[1],
            "repos/owner/repo/statuses/0123456789abcdef0123456789abcdef01234567"
        );
        assert!(args.contains(&"state=pending".to_owned()));
        assert!(args.contains(&"context=dragonforge/test-lab".to_owned()));
    }

    #[test]
    fn validates_commit_sha_and_revision() {
        assert!(validate_commit_sha("0123456789abcdef0123456789abcdef01234567").is_ok());
        assert!(validate_commit_sha("short").is_err());
        assert!(validate_revision("phase-2/github").is_ok());
        assert!(validate_revision("-bad").is_err());
        assert!(validate_revision("bad;whoami").is_err());
    }
}
