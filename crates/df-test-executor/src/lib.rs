use df_test_protocol::{JobRequest, JobStatus, ResourceLimits, TestAction};
use df_test_sandbox::{
    sandbox_project_command, verify_worker_identity, ProcessTreeGuard, SandboxError, SandboxLimits,
    SandboxMode,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use thiserror::Error;
use uuid::Uuid;

const DEFAULT_OUTPUT_LIMIT_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_POLL_INTERVAL_MS: u64 = 50;

#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    pub workspace_root: PathBuf,
    pub artifact_root: PathBuf,
    pub output_limit_bytes: usize,
    pub poll_interval: Duration,
    pub retain_workspace: bool,
    pub sandbox_mode: SandboxMode,
    pub expected_worker_user: Option<String>,
}

impl ExecutorConfig {
    pub fn under(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            workspace_root: root.join("workspaces"),
            artifact_root: root.join("artifacts"),
            output_limit_bytes: DEFAULT_OUTPUT_LIMIT_BYTES,
            poll_interval: Duration::from_millis(DEFAULT_POLL_INTERVAL_MS),
            retain_workspace: false,
            sandbox_mode: SandboxMode::Native,
            expected_worker_user: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepReport {
    pub name: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u128,
    pub stdout: String,
    pub stderr: String,
    pub output_truncated: bool,
    pub status: StepStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Passed,
    Failed,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReport {
    pub job_id: Uuid,
    pub status: JobStatus,
    pub summary: String,
    pub steps: Vec<StepReport>,
    pub artifact_directory: String,
    pub sandbox_mode: String,
}

pub struct LocalExecutor {
    config: ExecutorConfig,
}

impl LocalExecutor {
    pub fn new(config: ExecutorConfig) -> Self {
        Self { config }
    }

    pub fn execute(
        &self,
        job: &JobRequest,
        cancellation: &CancellationToken,
    ) -> Result<ExecutionReport, ExecutorError> {
        if let Some(expected) = &self.config.expected_worker_user {
            verify_worker_identity(expected)?;
        }

        fs::create_dir_all(&self.config.workspace_root)?;
        fs::create_dir_all(&self.config.artifact_root)?;

        let workspace_path = self.config.workspace_root.join(job.id.to_string());
        let artifact_path = self.config.artifact_root.join(job.id.to_string());

        if workspace_path.exists() {
            fs::remove_dir_all(&workspace_path)?;
        }
        if artifact_path.exists() {
            fs::remove_dir_all(&artifact_path)?;
        }

        fs::create_dir_all(&workspace_path)?;
        fs::create_dir_all(&artifact_path)?;

        let workspace = absolute_path(&workspace_path)?;
        let artifact_dir = absolute_path(&artifact_path)?;
        let repository_dir = workspace.join("repository");

        let deadline = Instant::now() + Duration::from_secs(job.limits.timeout_seconds);
        let mut steps = Vec::new();
        let mut checkout_ready = false;
        let mut final_status = JobStatus::Passed;

        for action in &job.actions {
            if cancellation.is_cancelled() {
                final_status = JobStatus::Cancelled;
                break;
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                final_status = JobStatus::Failed;
                steps.push(StepReport {
                    name: action_name(action).to_owned(),
                    exit_code: None,
                    duration_ms: 0,
                    stdout: String::new(),
                    stderr: "job timeout reached before step started".into(),
                    output_truncated: false,
                    status: StepStatus::TimedOut,
                });
                break;
            }

            match action {
                TestAction::Checkout => {
                    let clone_step = self.run_command(
                        CommandSpec::git_clone(&job.repository.url, &repository_dir),
                        remaining,
                        cancellation,
                        &job.limits,
                        false,
                    )?;
                    self.write_step_logs(&artifact_dir, steps.len(), &clone_step)?;
                    let clone_passed = clone_step.status == StepStatus::Passed;
                    final_status = status_from_step(clone_step.status);
                    steps.push(clone_step);

                    if !clone_passed {
                        break;
                    }

                    validate_revision(&job.repository.revision)?;

                    let remaining = deadline.saturating_duration_since(Instant::now());
                    let fetch_step = self.run_command(
                        CommandSpec::git_fetch_revision(&repository_dir, &job.repository.revision),
                        remaining,
                        cancellation,
                        &job.limits,
                        false,
                    )?;
                    self.write_step_logs(&artifact_dir, steps.len(), &fetch_step)?;
                    let fetch_passed = fetch_step.status == StepStatus::Passed;
                    final_status = status_from_step(fetch_step.status);
                    steps.push(fetch_step);

                    if !fetch_passed {
                        break;
                    }

                    let remaining = deadline.saturating_duration_since(Instant::now());
                    let checkout_step = self.run_command(
                        CommandSpec::git_switch_fetch_head(&repository_dir),
                        remaining,
                        cancellation,
                        &job.limits,
                        false,
                    )?;
                    self.write_step_logs(&artifact_dir, steps.len(), &checkout_step)?;
                    let checkout_passed = checkout_step.status == StepStatus::Passed;
                    final_status = status_from_step(checkout_step.status);
                    steps.push(checkout_step);

                    if !checkout_passed {
                        break;
                    }

                    checkout_ready = true;
                    self.enforce_disk_limit(&workspace, job.limits.max_disk_mib)?;
                    final_status = JobStatus::Passed;
                }
                _ => {
                    if !checkout_ready {
                        return Err(ExecutorError::RepositoryNotCheckedOut);
                    }

                    let spec = CommandSpec::for_action(action, &repository_dir)
                        .ok_or(ExecutorError::UnsupportedAction)?;
                    let step =
                        self.run_command(spec, remaining, cancellation, &job.limits, true)?;
                    self.write_step_logs(&artifact_dir, steps.len(), &step)?;
                    self.enforce_disk_limit(&workspace, job.limits.max_disk_mib)?;
                    final_status = status_from_step(step.status);
                    let passed = step.status == StepStatus::Passed;
                    steps.push(step);

                    if !passed {
                        break;
                    }
                }
            }
        }

        if final_status == JobStatus::Passed && steps.is_empty() {
            final_status = JobStatus::Failed;
        }

        let summary = match final_status {
            JobStatus::Passed => format!("{} step(s) passed", steps.len()),
            JobStatus::Cancelled => "job cancelled".to_owned(),
            JobStatus::Failed => "one or more steps failed".to_owned(),
            _ => format!("job finished with status {final_status:?}"),
        };

        let report = ExecutionReport {
            job_id: job.id,
            status: final_status,
            summary,
            steps,
            artifact_directory: artifact_dir.to_string_lossy().into_owned(),
            sandbox_mode: self.config.sandbox_mode.as_str().to_owned(),
        };

        fs::write(
            artifact_dir.join("report.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;

        if !self.config.retain_workspace && workspace.exists() {
            fs::remove_dir_all(&workspace)?;
        }

        Ok(report)
    }

    pub fn cleanup_job(&self, job_id: Uuid) -> Result<(), ExecutorError> {
        let workspace = self.config.workspace_root.join(job_id.to_string());
        if workspace.exists() {
            fs::remove_dir_all(workspace)?;
        }
        Ok(())
    }

    fn run_command(
        &self,
        spec: CommandSpec,
        timeout: Duration,
        cancellation: &CancellationToken,
        limits: &ResourceLimits,
        project_action: bool,
    ) -> Result<StepReport, ExecutorError> {
        let started = Instant::now();
        let sandbox_limits = SandboxLimits {
            max_memory_mib: limits.max_memory_mib,
            max_processes: limits.max_processes,
        };

        let effective = if project_action {
            let wrapped = sandbox_project_command(
                self.config.sandbox_mode,
                &spec.program,
                &spec.args,
                &spec.current_dir,
                sandbox_limits,
            )?;
            CommandSpec {
                name: spec.name,
                program: wrapped.program,
                args: wrapped.args,
                current_dir: wrapped.current_dir,
            }
        } else {
            spec
        };

        let guard = ProcessTreeGuard::new(self.config.sandbox_mode, sandbox_limits)?;
        let mut command = Command::new(&effective.program);
        command
            .args(&effective.args)
            .current_dir(&effective.current_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        apply_sanitized_environment(&mut command);

        let mut child = command.spawn().map_err(|source| ExecutorError::Spawn {
            program: effective.program.clone(),
            source,
        })?;
        guard.attach(&child)?;

        let stdout = child.stdout.take().ok_or(ExecutorError::MissingPipe)?;
        let stderr = child.stderr.take().ok_or(ExecutorError::MissingPipe)?;
        let output_limit = self.config.output_limit_bytes;

        let stdout_reader = thread::spawn(move || read_bounded(stdout, output_limit));
        let stderr_reader = thread::spawn(move || read_bounded(stderr, output_limit));

        let (status, step_status) = wait_for_child(
            &mut child,
            timeout,
            self.config.poll_interval,
            cancellation,
            &guard,
        )?;

        let (stdout_bytes, stdout_truncated) = stdout_reader
            .join()
            .map_err(|_| ExecutorError::ReaderPanicked)??;
        let (stderr_bytes, stderr_truncated) = stderr_reader
            .join()
            .map_err(|_| ExecutorError::ReaderPanicked)??;

        Ok(StepReport {
            name: effective.name,
            exit_code: status.and_then(|value| value.code()),
            duration_ms: started.elapsed().as_millis(),
            stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
            stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
            output_truncated: stdout_truncated || stderr_truncated,
            status: step_status,
        })
    }

    fn write_step_logs(
        &self,
        artifact_dir: &Path,
        index: usize,
        step: &StepReport,
    ) -> Result<(), ExecutorError> {
        let safe_name = step
            .name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>();

        fs::write(
            artifact_dir.join(format!("{index:02}-{safe_name}.stdout.log")),
            step.stdout.as_bytes(),
        )?;
        fs::write(
            artifact_dir.join(format!("{index:02}-{safe_name}.stderr.log")),
            step.stderr.as_bytes(),
        )?;
        Ok(())
    }

    fn enforce_disk_limit(&self, workspace: &Path, max_disk_mib: u64) -> Result<(), ExecutorError> {
        let used = directory_size(workspace)?;
        let limit = max_disk_mib.saturating_mul(1024 * 1024);
        if used > limit {
            return Err(ExecutorError::DiskLimitExceeded { used, limit });
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct CommandSpec {
    name: String,
    program: String,
    args: Vec<String>,
    current_dir: PathBuf,
}

impl CommandSpec {
    fn git_clone(url: &str, repository_dir: &Path) -> Self {
        let current_dir = repository_dir
            .parent()
            .expect("repository directory must have a workspace parent")
            .to_path_buf();
        debug_assert!(current_dir.is_absolute());
        debug_assert!(repository_dir.is_absolute());
        Self {
            name: "git_clone".into(),
            program: "git".into(),
            args: vec![
                "clone".into(),
                "--no-checkout".into(),
                "--".into(),
                url.into(),
                repository_dir.to_string_lossy().into_owned(),
            ],
            current_dir,
        }
    }

    fn git_fetch_revision(repository_dir: &Path, revision: &str) -> Self {
        Self {
            name: "git_fetch_revision".into(),
            program: "git".into(),
            args: vec![
                "fetch".into(),
                "--no-tags".into(),
                "origin".into(),
                revision.into(),
            ],
            current_dir: repository_dir.to_path_buf(),
        }
    }

    fn git_switch_fetch_head(repository_dir: &Path) -> Self {
        Self {
            name: "git_switch_detached".into(),
            program: "git".into(),
            args: vec!["switch".into(), "--detach".into(), "FETCH_HEAD".into()],
            current_dir: repository_dir.to_path_buf(),
        }
    }

    fn for_action(action: &TestAction, repository_dir: &Path) -> Option<Self> {
        let (name, args) = match action {
            TestAction::CargoBuild { release } => {
                let mut args = vec!["build".into(), "--workspace".into()];
                if *release {
                    args.push("--release".into());
                }
                ("cargo_build", args)
            }
            TestAction::CargoTest { all_features } => {
                let mut args = vec!["test".into(), "--workspace".into()];
                if *all_features {
                    args.push("--all-features".into());
                }
                ("cargo_test", args)
            }
            TestAction::CargoClippy { deny_warnings } => {
                let mut args = vec![
                    "clippy".into(),
                    "--workspace".into(),
                    "--all-targets".into(),
                    "--all-features".into(),
                ];
                if *deny_warnings {
                    args.extend(["--".into(), "-D".into(), "warnings".into()]);
                }
                ("cargo_clippy", args)
            }
            TestAction::CargoFmtCheck => (
                "cargo_fmt_check",
                vec!["fmt".into(), "--all".into(), "--".into(), "--check".into()],
            ),
            TestAction::Checkout => return None,
        };

        Some(Self {
            name: name.into(),
            program: "cargo".into(),
            args,
            current_dir: repository_dir.to_path_buf(),
        })
    }
}

fn validate_revision(revision: &str) -> Result<(), ExecutorError> {
    let valid = !revision.is_empty()
        && !revision.starts_with('-')
        && revision.len() <= 256
        && revision
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'));

    if valid {
        Ok(())
    } else {
        Err(ExecutorError::UnsafeRevision)
    }
}

fn status_from_step(status: StepStatus) -> JobStatus {
    match status {
        StepStatus::Passed => JobStatus::Passed,
        StepStatus::Cancelled => JobStatus::Cancelled,
        StepStatus::Failed | StepStatus::TimedOut => JobStatus::Failed,
    }
}

fn wait_for_child(
    child: &mut Child,
    timeout: Duration,
    poll_interval: Duration,
    cancellation: &CancellationToken,
    guard: &ProcessTreeGuard,
) -> Result<(Option<ExitStatus>, StepStatus), ExecutorError> {
    let started = Instant::now();

    loop {
        if cancellation.is_cancelled() {
            terminate_child(child, guard)?;
            return Ok((None, StepStatus::Cancelled));
        }

        if started.elapsed() >= timeout {
            terminate_child(child, guard)?;
            return Ok((None, StepStatus::TimedOut));
        }

        if let Some(status) = child.try_wait()? {
            let step_status = if status.success() {
                StepStatus::Passed
            } else {
                StepStatus::Failed
            };
            return Ok((Some(status), step_status));
        }

        thread::sleep(poll_interval);
    }
}

fn terminate_child(
    child: &mut Child,
    guard: &ProcessTreeGuard,
) -> Result<(), ExecutorError> {
    if guard.terminate_tree()? {
        let _ = child.wait();
        return Ok(());
    }

    match child.kill() {
        Ok(()) => {
            let _ = child.wait();
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::InvalidInput => Ok(()),
        Err(error) => Err(ExecutorError::Io(error)),
    }
}

fn read_bounded(mut reader: impl Read, limit: usize) -> io::Result<(Vec<u8>, bool)> {
    let mut retained = Vec::with_capacity(limit.min(64 * 1024));
    let mut buffer = [0_u8; 16 * 1024];
    let mut truncated = false;

    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }

        if retained.len() < limit {
            let remaining = limit - retained.len();
            let keep = remaining.min(read);
            retained.extend_from_slice(&buffer[..keep]);
            if keep < read {
                truncated = true;
            }
        } else {
            truncated = true;
        }
    }

    Ok((retained, truncated))
}

fn apply_sanitized_environment(command: &mut Command) {
    const ALLOWED: &[&str] = &[
        "PATH",
        "Path",
        "SYSTEMROOT",
        "SystemRoot",
        "WINDIR",
        "TEMP",
        "TMP",
        "HOME",
        "USERPROFILE",
        "CARGO_HOME",
        "RUSTUP_HOME",
        "LANG",
        "LC_ALL",
        "TERM",
    ];

    let current: BTreeMap<String, String> = std::env::vars().collect();
    command.env_clear();

    for key in ALLOWED {
        if let Some(value) = current.get(*key) {
            command.env(key, value);
        }
    }
}

fn absolute_path(path: &Path) -> io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn directory_size(root: &Path) -> io::Result<u64> {
    let mut total = 0_u64;
    let mut pending = vec![root.to_path_buf()];

    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                total = total.saturating_add(entry.metadata()?.len());
            }
        }
    }

    Ok(total)
}

fn action_name(action: &TestAction) -> &'static str {
    match action {
        TestAction::Checkout => "checkout",
        TestAction::CargoBuild { .. } => "cargo_build",
        TestAction::CargoTest { .. } => "cargo_test",
        TestAction::CargoClippy { .. } => "cargo_clippy",
        TestAction::CargoFmtCheck => "cargo_fmt_check",
    }
}

#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error("failed to spawn {program}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: io::Error,
    },
    #[error("repository must be checked out before Cargo actions can run")]
    RepositoryNotCheckedOut,
    #[error("unsupported test action")]
    UnsupportedAction,
    #[error("repository revision contains unsafe characters or begins with '-'")]
    UnsafeRevision,
    #[error("child process output pipe was unavailable")]
    MissingPipe,
    #[error("output reader thread panicked")]
    ReaderPanicked,
    #[error("workspace disk limit exceeded: used {used} bytes, limit {limit} bytes")]
    DiskLimitExceeded { used: u64, limit: u64 },
    #[error(transparent)]
    Sandbox(#[from] SandboxError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_token_can_be_shared() {
        let first = CancellationToken::new();
        let second = first.clone();
        assert!(!second.is_cancelled());
        first.cancel();
        assert!(second.is_cancelled());
    }

    #[test]
    fn cargo_commands_are_fixed_and_do_not_use_shells() {
        let root = Path::new("repo");
        let spec = CommandSpec::for_action(
            &TestAction::CargoClippy {
                deny_warnings: true,
            },
            root,
        )
        .unwrap();

        assert_eq!(spec.program, "cargo");
        assert_eq!(
            spec.args,
            vec![
                "clippy",
                "--workspace",
                "--all-targets",
                "--all-features",
                "--",
                "-D",
                "warnings"
            ]
        );
    }

    #[test]
    fn bounded_reader_drains_but_retains_only_limit() {
        let input = vec![b'x'; 1024];
        let (retained, truncated) = read_bounded(input.as_slice(), 128).unwrap();
        assert_eq!(retained.len(), 128);
        assert!(truncated);
    }

    #[test]
    fn git_clone_uses_workspace_as_absolute_working_directory() {
        let root = std::env::current_dir().unwrap();
        let repository_dir = root.join("workspace").join("repository");
        let spec =
            CommandSpec::git_clone("https://github.com/example/project.git", &repository_dir);

        assert_eq!(spec.current_dir, root.join("workspace"));
        assert!(spec.current_dir.is_absolute());
        assert_eq!(
            spec.args.last().unwrap(),
            &repository_dir.to_string_lossy().into_owned()
        );
    }

    #[test]
    fn absolute_path_does_not_require_canonicalization() {
        let relative = Path::new(".dragonforge-test-lab").join("workspaces");
        let absolute = absolute_path(&relative).unwrap();
        assert!(absolute.is_absolute());

        #[cfg(windows)]
        assert!(!absolute.to_string_lossy().starts_with(r"\\?\"));
    }

    #[test]
    fn remote_revision_flow_fetches_then_switches_fetch_head() {
        let repo = Path::new("repo");
        let fetch = CommandSpec::git_fetch_revision(repo, "phase-1-local-worker");
        assert_eq!(
            fetch.args,
            vec!["fetch", "--no-tags", "origin", "phase-1-local-worker"]
        );

        let switch = CommandSpec::git_switch_fetch_head(repo);
        assert_eq!(switch.args, vec!["switch", "--detach", "FETCH_HEAD"]);
    }

    #[test]
    fn executor_defaults_to_native_sandboxing() {
        let config = ExecutorConfig::under("lab");
        assert_eq!(config.sandbox_mode, SandboxMode::Native);
        assert!(config.expected_worker_user.is_none());
    }

    #[test]
    fn revision_validation_rejects_option_like_values() {
        assert!(validate_revision("main").is_ok());
        assert!(validate_revision("feature/test-1").is_ok());
        assert!(matches!(
            validate_revision("-dangerous-looking-revision"),
            Err(ExecutorError::UnsafeRevision)
        ));
        assert!(matches!(
            validate_revision("main;whoami"),
            Err(ExecutorError::UnsafeRevision)
        ));
    }
}
