use df_test_agent::Agent;
use df_test_distributed::{
    connect_registration_probe, run_distributed_fixtures, serve_registration_probe_once,
    validate_controller_addr, NodeFeature, NodeProfile, NodeRegistration,
};
use df_test_executor::{CancellationToken, ExecutionReport, ExecutorConfig, LocalExecutor};
use df_test_github::{CommitStatus, CommitStatusState, GhGitHubClient, GitHubRepository};
use df_test_gui::{GuiAutomationClient, GuiPlan};
use df_test_policy::ExecutionPolicy;
use df_test_protocol::{
    Capability, JobRequest, JobStatus, RepositorySpec, ResourceLimits, TestAction,
    WorkerRegistration, PROTOCOL_VERSION,
};
use df_test_sandbox::{
    current_worker_identity, runtime_version, verify_container_image, verify_worker_identity,
    ProcessTreeGuard, SandboxLimits, SandboxMode,
};
use df_test_vm::{GuestOs, HyperVClient, VmCreateSpec, VmLabConfig, DEFAULT_BASELINE_CHECKPOINT};
use df_test_windows::WindowsIntegrationClient;
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
        "distributed-doctor" => distributed_doctor(),
        "distributed-fixtures" => distributed_fixtures(),
        "distributed-controller-once" => distributed_controller_once(&args[2..]),
        "distributed-node-connect" => distributed_node_connect(&args[2..]),
        "gui-doctor" => gui_doctor(),
        "gui-run-plan" => gui_run_plan(&args[2..]),
        "gui-fixture" => gui_fixture(&args[2..]),
        "sandbox-doctor" => sandbox_doctor(&args[2..]),
        "rust-doctor" => rust_doctor(),
        "windows-doctor" => windows_doctor(),
        "windows-fixtures" => windows_fixtures(),
        "windows-privileged-fixtures" => windows_privileged_fixtures(&args[2..]),
        "windows-installer-info" => windows_installer_info(&args[2..]),
        "vm-doctor" => vm_doctor(&args[2..]),
        "vm-list" => vm_list(&args[2..]),
        "vm-create" => vm_create(&args[2..]),
        "vm-start" => vm_start(&args[2..]),
        "vm-stop" => vm_stop(&args[2..]),
        "vm-baseline" => vm_baseline(&args[2..]),
        "vm-restore" => vm_restore(&args[2..]),
        "vm-destroy" => vm_destroy(&args[2..]),
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
    println!("phase=8");

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

fn distributed_doctor() -> Result<(), Box<dyn std::error::Error>> {
    let loopback: std::net::SocketAddr = "127.0.0.1:1".parse()?;
    let private: std::net::SocketAddr = "10.0.0.1:1".parse()?;
    let public: std::net::SocketAddr = "8.8.8.8:53".parse()?;

    validate_controller_addr(loopback)?;
    validate_controller_addr(private)?;
    if validate_controller_addr(public).is_ok() {
        return Err("public controller address policy unexpectedly allowed a public IP".into());
    }

    println!("DragonForge Test Lab distributed doctor");
    println!("node_transport=outbound_only");
    println!("authentication=hmac_sha256");
    println!("replay_protection=nonce_and_clock_window");
    println!("controller_address_policy=loopback_private_link_local");
    println!("lease_health=enabled");
    println!("capability_scheduler=enabled");
    println!("result_artifact_hashing=sha256");
    println!("status=distributed_lab_ready");
    Ok(())
}

fn distributed_fixtures() -> Result<(), Box<dyn std::error::Error>> {
    let report = run_distributed_fixtures()?;
    println!("{}", serde_json::to_string_pretty(&report)?);

    if !report.registration_authenticated
        || !report.replay_rejected
        || !report.heartbeat_applied
        || !report.distinct_role_assignment
        || !report.artifact_hash_verified
        || !report.outbound_transport_round_trip
        || !report.network.tcp_loopback
        || !report.network.udp_loopback
        || !report.network.dns_localhost
        || !report.network.fault_drop_observed
    {
        return Err("one or more distributed/network fixtures failed".into());
    }

    println!("status=distributed_fixtures_passed");
    Ok(())
}

fn distributed_controller_once(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let bind = value_after(args, "--bind").ok_or("missing --bind <private-ip:port>")?;
    let key_id = value_after(args, "--key-id").ok_or("missing --key-id <id>")?;
    let address: std::net::SocketAddr = bind.parse()?;
    let secret = distributed_shared_secret()?;
    let now = unix_time_secs()?;

    println!("DragonForge Test Lab distributed controller registration probe");
    println!("bind={address}");
    println!("waiting_for=one outbound authenticated node registration");

    let probe = serve_registration_probe_once(
        address,
        &key_id,
        secret.as_bytes(),
        now,
        std::time::Duration::from_secs(120),
    )?;

    println!("node_id={}", probe.registration.profile.node_id);
    println!("node_os={}", probe.registration.profile.os);
    println!("node_arch={}", probe.registration.profile.arch);
    println!("job_id={}", probe.result.job_id);
    println!("job_status={:?}", probe.result.status);
    println!("artifacts={}", probe.result.artifacts.len());
    println!("status=distributed_cross_node_fixture_passed");
    Ok(())
}

fn distributed_node_connect(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let controller =
        value_after(args, "--controller").ok_or("missing --controller <private-ip:port>")?;
    let node_id = value_after(args, "--node-id").ok_or("missing --node-id <id>")?;
    let key_id = value_after(args, "--key-id").ok_or("missing --key-id <id>")?;
    let address: std::net::SocketAddr = controller.parse()?;
    let secret = distributed_shared_secret()?;
    let now = unix_time_secs()?;

    let mut features = BTreeSet::from([
        NodeFeature::Rust,
        NodeFeature::TcpFixture,
        NodeFeature::UdpFixture,
        NodeFeature::DnsFixture,
        NodeFeature::FaultInjection,
    ]);
    if cfg!(windows) {
        features.insert(NodeFeature::WindowsIntegration);
        features.insert(NodeFeature::GuiAutomation);
    }

    let registration = NodeRegistration {
        protocol_version: PROTOCOL_VERSION,
        profile: NodeProfile {
            node_id,
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            labels: ["phase8-probe".to_string()].into_iter().collect(),
            features,
            max_parallel_jobs: 2,
        },
        outbound_only: true,
        key_id: key_id.clone(),
    };

    let probe = connect_registration_probe(
        address,
        registration,
        &key_id,
        secret.as_bytes(),
        now,
        std::time::Duration::from_secs(10),
    )?;

    println!("controller={address}");
    println!("node_id={}", probe.ack.node_id);
    println!("lease_seconds={}", probe.ack.lease_seconds);
    println!("job_id={}", probe.result.job_id);
    println!("job_status={:?}", probe.result.status);
    println!("artifacts={}", probe.result.artifacts.len());
    println!("status=distributed_cross_node_fixture_passed");
    Ok(())
}

fn distributed_shared_secret() -> Result<String, Box<dyn std::error::Error>> {
    let secret = std::env::var("DRAGONFORGE_NODE_SHARED_SECRET")
        .map_err(|_| "DRAGONFORGE_NODE_SHARED_SECRET must be set and at least 32 bytes")?;
    if secret.len() < 32 {
        return Err("DRAGONFORGE_NODE_SHARED_SECRET must be at least 32 bytes".into());
    }
    Ok(secret)
}

fn unix_time_secs() -> Result<u64, Box<dyn std::error::Error>> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs())
}

fn gui_doctor() -> Result<(), Box<dyn std::error::Error>> {
    let report = GuiAutomationClient.doctor()?;
    println!("DragonForge Test Lab GUI doctor");
    println!("{}", serde_json::to_string_pretty(&report)?);

    if !report.user_interactive || !report.ui_automation_available || !report.drawing_available {
        return Err("interactive desktop, UI Automation, and drawing support are required".into());
    }

    println!("status=gui_automation_ready");
    Ok(())
}

fn gui_run_plan(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let plan_path = value_after(args, "--plan").ok_or("missing --plan <plan.json>")?;
    let artifact_dir = value_after(args, "--artifact-dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".dragonforge-test-lab").join("gui-artifacts"));
    let plan: GuiPlan = serde_json::from_slice(&std::fs::read(plan_path)?)?;
    let report = GuiAutomationClient.run_plan(&plan, &artifact_dir)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.passed {
        return Err("GUI plan failed".into());
    }
    Ok(())
}

fn gui_fixture(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let artifact_dir = value_after(args, "--artifact-dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".dragonforge-test-lab").join("gui-artifacts"));
    let fixture_script = PathBuf::from("scripts").join("phase7-gui-fixture.ps1");
    let (report, crash) = GuiAutomationClient.run_phase7_fixture(&fixture_script, &artifact_dir)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    println!("{}", serde_json::to_string_pretty(&crash)?);
    if !report.passed || !crash.captured {
        return Err("Phase 7 GUI fixture failed".into());
    }
    println!("status=gui_fixture_passed");
    Ok(())
}

fn rust_doctor() -> Result<(), Box<dyn std::error::Error>> {
    println!("DragonForge Test Lab deep Rust doctor");

    let nextest = tool_version("cargo", &["nextest", "--version"]);
    let llvm_cov = tool_version("cargo", &["llvm-cov", "--version"]);
    let fuzz = tool_version("cargo", &["fuzz", "--version"]);
    let miri = tool_version("cargo", &["+nightly", "miri", "--version"]);

    println!(
        "cargo_nextest={}",
        nextest.as_deref().unwrap_or("unavailable")
    );
    println!(
        "cargo_llvm_cov={}",
        llvm_cov.as_deref().unwrap_or("unavailable")
    );
    println!("cargo_fuzz={}", fuzz.as_deref().unwrap_or("unavailable"));
    println!("miri={}", miri.as_deref().unwrap_or("unavailable"));

    if nextest.is_none() || llvm_cov.is_none() {
        return Err(
            "Phase 5 requires cargo-nextest and cargo-llvm-cov; see docs/PHASE-5.md".into(),
        );
    }

    println!("status=deep_rust_ready");
    Ok(())
}

fn windows_doctor() -> Result<(), Box<dyn std::error::Error>> {
    let report = WindowsIntegrationClient.doctor()?;
    println!("DragonForge Test Lab Windows doctor");
    println!("{}", serde_json::to_string_pretty(&report)?);

    if !report.event_log_readable
        || !report.registry_hkcu_readable
        || !report.scm_readable
        || !report.windows_installer_service_present
        || !report.msiexec_present
    {
        return Err("one or more required Windows integration surfaces are unavailable".into());
    }

    println!("status=windows_integration_ready");
    Ok(())
}

fn windows_fixtures() -> Result<(), Box<dyn std::error::Error>> {
    let report = WindowsIntegrationClient.run_safe_fixtures()?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    println!("status=windows_safe_fixtures_passed");
    Ok(())
}

fn windows_privileged_fixtures(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if !args.iter().any(|arg| arg == "--confirm") {
        return Err("windows-privileged-fixtures requires --confirm".into());
    }

    let report = WindowsIntegrationClient.run_privileged_fixtures()?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    println!("status=windows_privileged_fixtures_passed");
    Ok(())
}

fn windows_installer_info(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = value_after(args, "--path").ok_or("missing --path <installer.msi>")?;
    let info = WindowsIntegrationClient.inspect_msi(&PathBuf::from(path))?;
    println!("{}", serde_json::to_string_pretty(&info)?);
    Ok(())
}

fn run_local(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let repo = value_after(args, "--repo").ok_or("missing --repo <https-url>")?;
    let revision = value_after(args, "--revision").unwrap_or_else(|| "main".into());
    let lab_root = lab_root(args);
    let retain_workspace = args.iter().any(|arg| arg == "--retain-workspace");
    let sandbox_mode = sandbox_mode(args)?;
    let worker_user = value_after(args, "--worker-user");

    let report = execute_local_job(
        repo,
        revision,
        lab_root,
        retain_workspace,
        sandbox_mode,
        worker_user,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);

    if report.status != JobStatus::Passed {
        std::process::exit(2);
    }

    Ok(())
}

fn run_github(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let repo_url = value_after(args, "--repo").ok_or("missing --repo <github-https-url>")?;
    let requested_revision = value_after(args, "--revision").unwrap_or_else(|| "main".into());
    let lab_root = lab_root(args);
    let retain_workspace = args.iter().any(|arg| arg == "--retain-workspace");
    let report_status = !args.iter().any(|arg| arg == "--no-status");
    let sandbox_mode = sandbox_mode(args)?;
    let worker_user = value_after(args, "--worker-user");

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
        sandbox_mode,
        worker_user,
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
    sandbox_mode: SandboxMode,
    expected_worker_user: Option<String>,
) -> Result<ExecutionReport, Box<dyn std::error::Error>> {
    if !repo.starts_with("https://") {
        return Err("--repo must be an HTTPS repository URL".into());
    }

    verify_container_image(sandbox_mode)?;

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
    config.sandbox_mode = sandbox_mode;
    config.expected_worker_user = expected_worker_user;
    let executor = LocalExecutor::new(config);
    let cancellation = CancellationToken::new();
    Ok(executor.execute(&job, &cancellation)?)
}

fn sandbox_doctor(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mode = sandbox_mode(args)?;
    let worker = current_worker_identity()?;

    if let Some(expected) = value_after(args, "--worker-user") {
        verify_worker_identity(&expected)?;
        println!("required_worker_user={expected}");
    }

    let defaults = ResourceLimits::default();
    let limits = SandboxLimits {
        max_memory_mib: defaults.max_memory_mib,
        max_processes: defaults.max_processes,
    };
    let guard = ProcessTreeGuard::new(mode, limits)?;

    println!("DragonForge Test Lab sandbox doctor");
    println!("sandbox_mode={}", mode.as_str());
    println!("worker_identity={worker}");
    println!("containment={}", guard.mechanism());
    println!("max_memory_mib={}", limits.max_memory_mib);
    println!("max_processes={}", limits.max_processes);

    if let Some(version) = runtime_version(mode)? {
        println!("container_runtime={version}");
        verify_container_image(mode)?;
        println!(
            "container_image={}",
            df_test_sandbox::DEFAULT_CONTAINER_IMAGE
        );
    }

    println!("status=sandbox_ready");
    Ok(())
}

fn vm_doctor(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let config = vm_config(args);
    let client = HyperVClient::default();
    let doctor = client.doctor()?;

    println!("DragonForge Test Lab VM doctor");
    println!("hyper_v_enabled={}", doctor.hyper_v_enabled);
    println!("hyper_v_module={}", doctor.hyper_v_module);
    println!("vmms_running={}", doctor.vmms_running);
    println!("vm_root={}", config.root.display());
    println!("base_image_root={}", config.base_image_root.display());
    println!("default_switch={}", config.switch_name);
    println!("switches={}", doctor.switches.join(","));

    if !doctor.hyper_v_enabled || !doctor.hyper_v_module || !doctor.vmms_running {
        return Err("Hyper-V host is not ready; see docs/HOST-SETUP-HYPERV.md".into());
    }

    if !doctor
        .switches
        .iter()
        .any(|name| name == &config.switch_name)
    {
        return Err(format!(
            "configured Hyper-V switch '{}' was not found",
            config.switch_name
        )
        .into());
    }

    println!("status=vm_lab_ready");
    Ok(())
}

fn vm_list(_args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let client = HyperVClient::default();
    println!("{}", serde_json::to_string_pretty(&client.list_managed()?)?);
    Ok(())
}

fn vm_create(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let config = vm_config(args);
    let name = value_after(args, "--name").ok_or("missing --name <DragonForge-...>")?;
    let base_vhdx = value_after(args, "--base-vhdx").ok_or("missing --base-vhdx <path>")?;
    let guest_os = GuestOs::parse(
        &value_after(args, "--guest-os").ok_or("missing --guest-os windows|linux")?,
    )?;
    let memory_mib = parse_u64_flag(args, "--memory-mib", 4096)?;
    let processors = parse_u32_flag(args, "--processors", 2)?;
    let switch_name = value_after(args, "--switch").unwrap_or_else(|| config.switch_name.clone());

    let spec = VmCreateSpec {
        name: name.clone(),
        guest_os,
        base_vhdx: PathBuf::from(base_vhdx),
        memory_mib,
        processors,
        switch_name,
    };

    let client = HyperVClient::default();
    client.create_from_base(&config, &spec)?;
    println!("vm_created={name}");
    println!("next_step=install/configure guest agent if needed, then run vm-baseline");
    Ok(())
}

fn vm_start(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let name = value_after(args, "--name").ok_or("missing --name <DragonForge-...>")?;
    HyperVClient::default().start(&name)?;
    println!("vm_started={name}");
    Ok(())
}

fn vm_stop(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let name = value_after(args, "--name").ok_or("missing --name <DragonForge-...>")?;
    HyperVClient::default().stop(&name)?;
    println!("vm_stopped={name}");
    Ok(())
}

fn vm_baseline(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let name = value_after(args, "--name").ok_or("missing --name <DragonForge-...>")?;
    HyperVClient::default().ensure_clean_baseline(&name)?;
    println!("vm_baseline={name}:{DEFAULT_BASELINE_CHECKPOINT}");
    Ok(())
}

fn vm_restore(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let name = value_after(args, "--name").ok_or("missing --name <DragonForge-...>")?;
    let checkpoint =
        value_after(args, "--checkpoint").unwrap_or_else(|| DEFAULT_BASELINE_CHECKPOINT.into());
    HyperVClient::default().restore(&name, &checkpoint)?;
    println!("vm_restored={name}:{checkpoint}");
    Ok(())
}

fn vm_destroy(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let config = vm_config(args);
    let name = value_after(args, "--name").ok_or("missing --name <DragonForge-...>")?;
    if !args.iter().any(|arg| arg == "--confirm") {
        return Err("vm-destroy requires --confirm".into());
    }
    HyperVClient::default().destroy_managed(&config, &name)?;
    println!("vm_destroyed={name}");
    Ok(())
}

fn vm_config(args: &[String]) -> VmLabConfig {
    let root = value_after(args, "--vm-root")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".dragonforge-test-lab").join("vm-lab"));
    let mut config = VmLabConfig::under(root);
    if let Some(value) = value_after(args, "--image-root") {
        config.base_image_root = PathBuf::from(value);
    }
    if let Some(value) = value_after(args, "--switch") {
        config.switch_name = value;
    }
    config
}

fn parse_u64_flag(
    args: &[String],
    flag: &str,
    default: u64,
) -> Result<u64, Box<dyn std::error::Error>> {
    match value_after(args, flag) {
        Some(value) => Ok(value.parse()?),
        None => Ok(default),
    }
}

fn parse_u32_flag(
    args: &[String],
    flag: &str,
    default: u32,
) -> Result<u32, Box<dyn std::error::Error>> {
    match value_after(args, flag) {
        Some(value) => Ok(value.parse()?),
        None => Ok(default),
    }
}

fn sandbox_mode(args: &[String]) -> Result<SandboxMode, Box<dyn std::error::Error>> {
    let value = value_after(args, "--sandbox").unwrap_or_else(|| "native".into());
    Ok(SandboxMode::parse(&value)?)
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
    println!("  dragonforge-test-lab distributed-doctor");
    println!("  dragonforge-test-lab distributed-fixtures");
    println!(
        "  dragonforge-test-lab distributed-controller-once --bind <private-ip:port> --key-id <id>"
    );
    println!("  dragonforge-test-lab distributed-node-connect --controller <private-ip:port> --node-id <id> --key-id <id>");
    println!("  dragonforge-test-lab gui-doctor");
    println!("  dragonforge-test-lab gui-run-plan --plan <plan.json> [--artifact-dir <path>]");
    println!("  dragonforge-test-lab gui-fixture [--artifact-dir <path>]");
    println!("  dragonforge-test-lab sandbox-doctor [--sandbox native|docker|podman] [--worker-user <name>]");
    println!("  dragonforge-test-lab rust-doctor");
    println!("  dragonforge-test-lab windows-doctor");
    println!("  dragonforge-test-lab windows-fixtures");
    println!("  dragonforge-test-lab windows-privileged-fixtures --confirm");
    println!("  dragonforge-test-lab windows-installer-info --path <installer.msi>");
    println!("  dragonforge-test-lab vm-doctor [--vm-root <path>] [--switch <name>]");
    println!("  dragonforge-test-lab vm-list");
    println!("  dragonforge-test-lab vm-create --name <DragonForge-...> --guest-os windows|linux --base-vhdx <path> [--memory-mib 4096] [--processors 2] [--switch <name>]");
    println!("  dragonforge-test-lab vm-start --name <DragonForge-...>");
    println!("  dragonforge-test-lab vm-stop --name <DragonForge-...>");
    println!("  dragonforge-test-lab vm-baseline --name <DragonForge-...>");
    println!("  dragonforge-test-lab vm-restore --name <DragonForge-...> [--checkpoint DragonForge-Baseline]");
    println!("  dragonforge-test-lab vm-destroy --name <DragonForge-...> --confirm");
    println!("  dragonforge-test-lab version");
    println!("  dragonforge-test-lab run-local --repo <https-url> [--revision <ref>] [--lab-root <path>] [--retain-workspace] [--sandbox native|docker|podman] [--worker-user <name>]");
    println!("  dragonforge-test-lab run-github --repo <github-https-url> [--revision <ref>] [--lab-root <path>] [--retain-workspace] [--sandbox native|docker|podman] [--worker-user <name>] [--no-status]");
}
