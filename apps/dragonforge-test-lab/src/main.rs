use df_test_agent::Agent;
use df_test_executor::{CancellationToken, ExecutionReport, ExecutorConfig, LocalExecutor};
use df_test_github::{
    CommitStatus, CommitStatusState, GhGitHubClient, GitHubRepository,
};
use df_test_policy::ExecutionPolicy;
use df_test_protocol::{
    Capability, JobRequest, JobStatus, RepositorySpec, TestAction, WorkerRegistration,
    PROTOCOL_VERSION,
};
use std::{collections::BTreeSet, path::PathBuf, process::Command};

const GITHUB_STATUS_CONTEXT: &str = "dragonforge/test-lab";

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(String::as_str).unwrap_or("help");

    match command {
        "doctor" => doctor(),
        "github-doctor" => github_doctor(),
        "run-local" => run_local(&args[2..]),
        "run-github" => run_github(&args[2..]),
        "--version" | "-V" | "version" => {
            println!("DragonForge Test Lab {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        _ => {
            print_help();
            Ok(())
        }
    }
}

fn doctor() -> Result<(), Box<dyn std::error::Error>> {
    println!("DragonForge Test Lab doctor");
    println!("version={}", env!("CARGO_PKG_VERSION"));
    println!("protocol_version={PROTOCOL_VERSION}");
    println!("os={}", std::env::consts::OS);
    println!("arch={}", std::env::consts::ARCH);
    println!("phase=2");

    let git = tool_version("git", &["--version"]);
    let cargo = tool_version("cargo", &["--version"]);
    let gh = tool_version("gh", &["--version"]);

    println!("git={}", git.as_deref().unwrap_or("unavailable"));
    println!("cargo={}", cargo.as_deref().unwrap_or("unavailable"));
    println!("gh={}", gh.as_deref().unwrap_or("unavailable"));

    if git.is_none() || cargo.is_none() {
        return Err("git and cargo must both be available on PATH".into());
    }

    println!("status=local_worker_ready");
    Ok(())
}

fn github_doctor() -> Result<(), Box<dyn std::error::Error>> {
    let client = GhGitHubClient::default();
    client.doctor()?;
    println!("DragonForge Test Lab GitHub doctor");
    println!("github_host=github.com");
    println!("status=github_ready");
    Ok(())
}

fn run_local(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let repo = value_after(args, "--repo").ok_or("missing --repo <https-url>")?;
    let revision = value_after(args, "--revision").unwrap_or_else(|| "main".into());
    let lab_root = lab_root(args);
    let retain_workspace = args.iter().any(|arg| arg == "--retain-workspace");

    let report = execute_local_job(repo, revision, lab_root, retain_workspace)?;
    println!("{}", serde_json::to_string_pretty(&report)?);

    if report.status != JobStatus::Passed {
        std::process::exit(2);
    }

    Ok(())
}

fn run_github(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let repo_url = value_after(args, "--repo").ok_or("missing --repo <github-https-url>")?;
    let requested_revision =
        value_after(args, "--revision").unwrap_or_else(|| "main".into());
    let lab_root = lab_root(args);
    let retain_workspace = args.iter().any(|arg| arg == "--retain-workspace");
    let report_status = !args.iter().any(|arg| arg == "--no-status");

    let repository = GitHubRepository::parse_https(&repo_url)?;
    let github = GhGitHubClient::default();
    github.doctor()?;
    let commit_sha = github.resolve_commit(&repository, &requested_revision)?;

    println!("github_repository={}", repository.slug());
    println!("requested_revision={requested_revision}");
    println!("resolved_commit={commit_sha}");

    if report_status {
        github.set_commit_status(
            &repository,
            &commit_sha,
            &CommitStatus::new(
                CommitStatusState::Pending,
                GITHUB_STATUS_CONTEXT,
                "DragonForge Test Lab validation is running",
            )?,
        )?;
    }

    let result = execute_local_job(
        repo_url,
        commit_sha.clone(),
        lab_root,
        retain_workspace,
    );

    match result {
        Ok(report) => {
            if report_status {
                let (state, description) = match report.status {
                    JobStatus::Passed => (
                        CommitStatusState::Success,
                        "DragonForge Test Lab validation passed",
                    ),
                    JobStatus::Cancelled => (
                        CommitStatusState::Error,
                        "DragonForge Test Lab validation was cancelled",
                    ),
                    _ => (
                        CommitStatusState::Failure,
                        "DragonForge Test Lab validation failed",
                    ),
                };

                github.set_commit_status(
                    &repository,
                    &commit_sha,
                    &CommitStatus::new(state, GITHUB_STATUS_CONTEXT, description)?,
                )?;
            }

            println!("{}", serde_json::to_string_pretty(&report)?);
            if report.status != JobStatus::Passed {
                std::process::exit(2);
            }
            Ok(())
        }
        Err(error) => {
            if report_status {
                let status = CommitStatus::new(
                    CommitStatusState::Error,
                    GITHUB_STATUS_CONTEXT,
                    "DragonForge Test Lab encountered an execution error",
                )?;
                if let Err(status_error) =
                    github.set_commit_status(&repository, &commit_sha, &status)
                {
                    eprintln!("warning: failed to report GitHub error status: {status_error}");
                }
            }
            Err(error)
        }
    }
}

fn execute_local_job(
    repo: String,
    revision: String,
    lab_root: PathBuf,
    retain_workspace: bool,
) -> Result<ExecutionReport, Box<dyn std::error::Error>> {
    if !repo.starts_with("https://") {
        return Err("--repo must be an HTTPS repository URL".into());
    }

    let capabilities: BTreeSet<Capability> = [
        Capability::CheckoutRepository,
        Capability::CargoBuild,
        Capability::CargoTest,
        Capability::CargoClippy,
        Capability::CargoFmtCheck,
        Capability::ReadArtifacts,
    ]
    .into_iter()
    .collect();

    let policy = ExecutionPolicy::new(vec![repo.clone()], capabilities.clone());
    let registration = WorkerRegistration {
        worker_id: format!("local-{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        protocol_version: PROTOCOL_VERSION,
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        capabilities,
    };

    let agent = Agent::new(registration, policy)?;
    let job = JobRequest::new(
        RepositorySpec {
            url: repo,
            revision,
        },
        vec![
            TestAction::Checkout,
            TestAction::CargoFmtCheck,
            TestAction::CargoClippy {
                deny_warnings: true,
            },
            TestAction::CargoTest { all_features: true },
        ],
    );

    agent.validate_job(&job)?;

    let mut config = ExecutorConfig::under(lab_root);
    config.retain_workspace = retain_workspace;
    let executor = LocalExecutor::new(config);
    let cancellation = CancellationToken::new();
    Ok(executor.execute(&job, &cancellation)?)
}

fn lab_root(args: &[String]) -> PathBuf {
    value_after(args, "--lab-root")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".dragonforge-test-lab"))
}

fn value_after(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn tool_version(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn print_help() {
    println!("DragonForge Test Lab");
    println!("Usage:");
    println!("  dragonforge-test-lab doctor");
    println!("  dragonforge-test-lab github-doctor");
    println!("  dragonforge-test-lab version");
    println!("  dragonforge-test-lab run-local --repo <https-url> [--revision <ref>] [--lab-root <path>] [--retain-workspace]");
    println!("  dragonforge-test-lab run-github --repo <github-https-url> [--revision <ref>] [--lab-root <path>] [--retain-workspace] [--no-status]");
}
