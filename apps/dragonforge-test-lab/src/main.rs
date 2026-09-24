use df_test_agent::Agent;
use df_test_arm::{run_phase20_fixture, ArmCapability, ArmInspector, HardwarePlan, HardwareProbe};
use df_test_chaos::{run_chaos_fixture, DEFAULT_STRESS_JOBS, MAX_STRESS_JOBS};
use df_test_dogfood::{
    load_campaign, load_profile, DogfoodCampaignReport, DogfoodProfile, DogfoodRunRecord,
    DOGFOOD_SCHEMA_VERSION, MAX_PROFILE_BYTES,
};
use df_test_controller::{DurableController, DurableJobState, SCHEMA_VERSION};
use df_test_dashboard::{
    run_dashboard_fixture, Dashboard, DashboardConfig, DEFAULT_DASHBOARD_BIND,
};
use df_test_distributed::{
    connect_registration_probe, run_distributed_fixtures, serve_registration_probe_once,
    validate_controller_addr, NodeFeature, NodeProfile, NodeRegistration,
};
use df_test_executor::{CancellationToken, ExecutionReport, ExecutorConfig, LocalExecutor};
use df_test_github::{CommitStatus, CommitStatusState, GhGitHubClient, GitHubRepository};
use df_test_gui::{GuiAutomationClient, GuiPlan};
use df_test_identity::{run_mtls_fixture, validate_private_controller_address};
use df_test_install::{
    InstallLayout, InstallPlatform, InstallState, ManagedInstallConfig, ReleaseManifest,
};
use df_test_intelligence::{analyze, IntelligenceInput, TestProfile, WorkerCapacity};
use df_test_intelligence_integration::{
    integrate, IntegrationRequest, IntelligenceMode, DEFAULT_MIN_AUTOMATIC_SCORE,
};
use df_test_lifecycle::{FailureClass, LifecycleDecision, RetryPolicy};
use df_test_mcp::{McpGateway, McpGatewayConfig};
use df_test_observability::{
    catalog_artifact, prune_artifacts, ArtifactRetentionPolicy, JsonlLogWriter, LogLevel,
    MetricPoint, MetricsRegistry, StructuredLogEvent,
};
use df_test_plans::{
    ArtifactKind, PlanCondition, PlanProfile, PlanStep, PlanStepStatus, TargetOs, TestPlan,
    TEST_PLAN_VERSION,
};
use df_test_policy::ExecutionPolicy;
use df_test_protocol::{
    Capability, JobRequest, JobStatus, RepositorySpec, ResourceLimits, TestAction,
    WorkerRegistration, PROTOCOL_VERSION,
};
use df_test_release::{
    ReleaseArtifact, ReleaseArtifactKind, ReleaseBundleManifest, ReleaseChannel, ReleaseVersion,
    RELEASE_BUNDLE_SCHEMA_VERSION,
};
use df_test_sandbox::{
    current_worker_identity, runtime_version, verify_container_image, verify_worker_identity,
    ProcessTreeGuard, SandboxLimits, SandboxMode,
};
use df_test_security_review::{run_fixture as run_security_fixture, run_security_review};
use df_test_vm::{GuestOs, HyperVClient, VmCreateSpec, VmLabConfig, DEFAULT_BASELINE_CHECKPOINT};
use df_test_windows::WindowsIntegrationClient;
use df_test_worker_service::{
    load_worker_config, run_worker_service_fixture, MtlsWorkerSession, SystemdServiceSpec,
    WindowsServiceSpec, WorkerServiceRuntime,
};
use std::{collections::BTreeSet, path::PathBuf, process::Command};

const GITHUB_STATUS_CONTEXT: &str = "dragonforge/test-lab";

#[cfg(windows)]
windows_service::define_windows_service!(ffi_worker_service_main, windows_worker_service_main);

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
        "controller-state-doctor" => controller_state_doctor(&args[2..]),
        "controller-state-fixture" => controller_state_fixture(),
        "dashboard-doctor" => dashboard_doctor(&args[2..]),
        "dashboard-fixture" => dashboard_fixture(),
        "dashboard-serve" => dashboard_serve(&args[2..]),
        "linux-doctor" => linux_doctor(),
        "linux-fixture" => linux_fixture(),
        "arm-doctor" => arm_doctor(),
        "arm-fixture" => arm_fixture(),
        "arm-probe" => arm_probe(&args[2..]),
        "install-doctor" => install_doctor(),
        "install-fixture" => install_fixture(),
        "install-layout" => install_layout(),
        "release-verify" => release_verify(&args[2..]),
        "upgrade-plan" => upgrade_plan(&args[2..]),
        "release-doctor" => release_doctor(),
        "release-fixture" => release_fixture(),
        "release-tag" => release_tag(&args[2..]),
        "release-bundle-verify" => release_bundle_verify(&args[2..]),
        "security-doctor" => security_doctor(),
        "security-fixture" => security_fixture(),
        "security-review" => security_review(&args[2..]),
        "chaos-doctor" => chaos_doctor(),
        "chaos-fixture" => chaos_fixture(&args[2..]),
        "dogfood-doctor" => dogfood_doctor(),
        "dogfood-fixture" => dogfood_fixture(),
        "dogfood-profile-validate" => dogfood_profile_validate(&args[2..]),
        "dogfood-profile-compile" => dogfood_profile_compile(&args[2..]),
        "dogfood-campaign-validate" => dogfood_campaign_validate(&args[2..]),
        "dogfood-campaign-run" => dogfood_campaign_run(&args[2..]),
        "dogfood-run" => dogfood_run(&args[2..]),
        "github-doctor" => github_doctor(),
        "identity-doctor" => identity_doctor(),
        "identity-fixture" => identity_fixture(),
        "worker-service-doctor" => worker_service_doctor(),
        "worker-service-fixture" => worker_service_fixture(),
        "worker-service-run" => worker_service_run(&args[2..]),
        "worker-service-windows" => worker_service_windows(&args[2..]),
        "worker-service-drain" => worker_service_drain(&args[2..]),
        "worker-service-resume" => worker_service_resume(&args[2..]),
        "worker-service-specs" => worker_service_specs(&args[2..]),
        "observability-doctor" => observability_doctor(),
        "observability-fixture" => observability_fixture(),
        "observability-summary" => observability_summary(&args[2..]),
        "lifecycle-doctor" => lifecycle_doctor(),
        "lifecycle-fixture" => lifecycle_fixture(),
        "lifecycle-status" => lifecycle_status(&args[2..]),
        "lifecycle-reschedule" => lifecycle_reschedule(&args[2..]),
        "plan-doctor" => plan_doctor(),
        "plan-fixture" => plan_fixture(),
        "plan-validate" => plan_validate(&args[2..]),
        "plan-compile" => plan_compile(&args[2..]),
        "plan-store" => plan_store(&args[2..]),
        "plan-list" => plan_list(&args[2..]),
        "distributed-doctor" => distributed_doctor(),
        "distributed-fixtures" => distributed_fixtures(),
        "distributed-controller-once" => distributed_controller_once(&args[2..]),
        "distributed-node-connect" => distributed_node_connect(&args[2..]),
        "mcp-doctor" => mcp_doctor(&args[2..]),
        "mcp-serve" => mcp_serve(&args[2..]),
        "mcp-fixture" => mcp_fixture(&args[2..]),
        "intelligence-doctor" => intelligence_doctor(),
        "intelligence-analyze" => intelligence_analyze(&args[2..]),
        "intelligence-fixture" => intelligence_fixture(),
        "intelligence-integration-doctor" => intelligence_integration_doctor(),
        "intelligence-integrate" => intelligence_integrate(&args[2..]),
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
    println!("phase=25");

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

fn controller_state_doctor(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = value_after(args, "--state-db")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".dragonforge-test-lab").join("controller.sqlite3"));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let controller = DurableController::open(&path)?;
    println!("DragonForge Test Lab durable controller doctor");
    println!("database={}", path.display());
    println!("schema_version={}", controller.schema_version()?);
    println!("expected_schema_version={SCHEMA_VERSION}");
    println!("persistence=sqlite");
    println!("restart_recovery=enabled");
    println!("status=durable_controller_ready");
    Ok(())
}

fn controller_state_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(format!(
        "dragonforge-phase11-fixture-{}.sqlite3",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);

    let job = JobRequest::new(
        RepositorySpec {
            url: "https://github.com/djames1987/DragonForge-Test-Lab.git".into(),
            revision: "main".into(),
        },
        vec![
            TestAction::Checkout,
            TestAction::CargoTest { all_features: true },
        ],
    );
    let worker = WorkerRegistration {
        worker_id: "phase11-fixture-worker".into(),
        protocol_version: PROTOCOL_VERSION,
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        capabilities: [Capability::CheckoutRepository, Capability::CargoTest]
            .into_iter()
            .collect(),
    };

    {
        let mut controller = DurableController::open(&path)?;
        controller.enqueue_job(&job, 1_000)?;
        controller.register_worker(&worker, 1_001)?;
        let assigned = controller
            .assign_next(&worker.worker_id, 1_002)?
            .ok_or("Phase 11 fixture did not assign persisted job")?;
        if assigned.id != job.id {
            return Err("Phase 11 fixture assigned unexpected job".into());
        }
        controller.mark_running(job.id, 1_003)?;
        controller.set_config("controller.fixture", "phase11", 1_004)?;
        controller.record_intelligence(
            &serde_json::json!({"fixture":"phase11","job_id":job.id}),
            1_005,
        )?;
    }

    let recovery = {
        let mut controller = DurableController::open(&path)?;
        let interrupted = controller.recover_after_restart(2_000)?;
        let record = controller
            .get_job(job.id)?
            .ok_or("Phase 11 persisted job disappeared after reopen")?;
        if interrupted != vec![job.id] || record.state != DurableJobState::Interrupted {
            return Err("Phase 11 restart recovery did not mark in-flight job interrupted".into());
        }
        if controller.get_config("controller.fixture")?.as_deref() != Some("phase11") {
            return Err("Phase 11 persisted controller configuration was not recovered".into());
        }
        let attempts = controller.list_attempts(job.id)?;
        let audit = controller.audit_events_for("job", &job.id.to_string())?;
        serde_json::json!({
            "database": path.display().to_string(),
            "schema_version": controller.schema_version()?,
            "job_id": job.id,
            "state": record.state,
            "attempts": attempts.len(),
            "audit_events": audit.len(),
            "interrupted_jobs": interrupted.len()
        })
    };

    println!("{}", serde_json::to_string_pretty(&recovery)?);
    println!("status=durable_controller_fixture_passed");
    let _ = std::fs::remove_file(path);
    Ok(())
}

fn dashboard_doctor(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let config = dashboard_config(args)?;
    let dashboard = Dashboard::new(config)?;
    let controller = DurableController::open(
        value_after(args, "--state-db")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".dragonforge-test-lab").join("controller.sqlite3")),
    )?;

    println!("DragonForge Test Lab dashboard doctor");
    println!("controller_schema={}", controller.schema_version()?);
    println!("bind={}", dashboard.bind_address());
    println!("loopback_only=true");
    println!("authentication=bearer_token_from_environment");
    println!("token_retained_in_plaintext=false");
    println!("read_only=true");
    println!("terminal_access=false");
    println!("raw_command_access=false");
    println!("raw_sql_access=false");
    println!("status=dashboard_ready");
    Ok(())
}

fn dashboard_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let report = run_dashboard_fixture()?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.loopback_enforced
        || !report.authentication_enforced
        || !report.overview_available
        || !report.jobs_available
        || !report.workers_available
        || !report.plans_available
        || !report.artifacts_available
        || !report.intelligence_available
        || !report.audit_available
        || !report.settings_safe
        || !report.mutating_methods_rejected
    {
        return Err("one or more Phase 18 dashboard fixtures failed".into());
    }
    println!("status=dashboard_fixture_passed");
    Ok(())
}

fn dashboard_serve(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let dashboard = Dashboard::new(dashboard_config(args)?)?;
    println!("DragonForge Test Lab dashboard");
    println!("bind={}", dashboard.bind_address());
    println!(
        "url=http://{}/#token=<DRAGONFORGE_DASHBOARD_TOKEN>",
        dashboard.bind_address()
    );
    println!("authentication=fragment_token_to_bearer_header");
    println!("read_only=true");
    println!("status=dashboard_listening");
    dashboard.serve()?;
    Ok(())
}

fn dashboard_config(args: &[String]) -> Result<DashboardConfig, Box<dyn std::error::Error>> {
    let bind: std::net::SocketAddr = value_after(args, "--bind")
        .unwrap_or_else(|| DEFAULT_DASHBOARD_BIND.into())
        .parse()?;
    let bearer_token = std::env::var("DRAGONFORGE_DASHBOARD_TOKEN")
        .map_err(|_| "DRAGONFORGE_DASHBOARD_TOKEN must be set and at least 32 characters")?;
    if bearer_token.len() < 32 {
        return Err("DRAGONFORGE_DASHBOARD_TOKEN must be at least 32 characters".into());
    }
    let state_db = value_after(args, "--state-db")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".dragonforge-test-lab").join("controller.sqlite3"));
    if let Some(parent) = state_db.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(DashboardConfig {
        bind,
        bearer_token,
        state_db,
    })
}

fn linux_doctor() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::consts::OS != "linux" {
        return Err("linux-doctor must run on a Linux host or Linux VM".into());
    }

    let limits = SandboxLimits {
        max_memory_mib: 512,
        max_processes: 8,
    };
    let guard = ProcessTreeGuard::new(SandboxMode::Native, limits)?;
    let systemd = SystemdServiceSpec::new(
        PathBuf::from("/opt/dragonforge/bin/dragonforge-test-lab"),
        PathBuf::from("/etc/dragonforge/test-worker.json"),
    )?;
    let unit = systemd.render_unit();

    println!("DragonForge Test Lab Linux doctor");
    println!("os={}", std::env::consts::OS);
    println!("arch={}", std::env::consts::ARCH);
    println!("native_containment={}", guard.mechanism());
    println!("systemd_unit={}", systemd.unit_name);
    println!(
        "systemd_no_new_privileges={}",
        unit.contains("NoNewPrivileges=true")
    );
    println!(
        "systemd_protect_system={}",
        unit.contains("ProtectSystem=strict")
    );
    println!("outbound_mtls=true");
    println!("container_modes=docker,podman");
    println!("status=linux_worker_ready");
    Ok(())
}

fn linux_fixture() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::consts::OS != "linux" {
        return Err("linux-fixture must run on a Linux host or Linux VM".into());
    }

    let guard = ProcessTreeGuard::new(
        SandboxMode::Native,
        SandboxLimits {
            max_memory_mib: 256,
            max_processes: 8,
        },
    )?;
    let mut command = Command::new("sleep");
    command.arg("30");
    command.stdin(std::process::Stdio::null());
    command.stdout(std::process::Stdio::null());
    command.stderr(std::process::Stdio::null());
    guard.prepare_command(&mut command)?;
    let mut child = command.spawn()?;
    guard.attach(&mut child)?;
    let cancellation_tree_killed = guard.terminate_tree()?;
    let _ = child.wait();

    let service = run_worker_service_fixture()?;
    let identity = run_mtls_fixture()?;
    let report = serde_json::json!({
        "native_containment": guard.mechanism() == "linux_process_group_rlimit",
        "cancellation_tree_killed": cancellation_tree_killed,
        "mtls_registration": service.mtls_registration,
        "heartbeat_received": service.heartbeat_received,
        "drain_blocks_new_jobs": service.drain_blocks_new_jobs,
        "restart_state_recovered": service.restart_state_recovered,
        "systemd_unit_valid": service.systemd_unit_valid,
        "encrypted_round_trip": identity.encrypted_round_trip,
        "node_identity_verified": identity.node_identity_verified
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report.as_object().is_none()
        || !cancellation_tree_killed
        || !service.mtls_registration
        || !service.heartbeat_received
        || !service.drain_blocks_new_jobs
        || !service.restart_state_recovered
        || !service.systemd_unit_valid
        || !identity.encrypted_round_trip
        || !identity.node_identity_verified
    {
        return Err("one or more Phase 19 Linux fixtures failed".into());
    }
    println!("status=linux_fixture_passed");
    Ok(())
}

fn arm_doctor() -> Result<(), Box<dyn std::error::Error>> {
    let inspector = arm_host_inspector()?;
    let inventory = inspector.inventory()?;
    let node_features = arm_node_features(&inventory);

    println!("DragonForge Test Lab ARM/Raspberry Pi doctor");
    println!("os={}", inventory.os);
    println!("arch={}", inventory.arch);
    println!(
        "board_model={}",
        inventory.board_model.as_deref().unwrap_or("unknown")
    );
    println!("arm_worker={}", inventory.is_arm());
    println!("raspberry_pi={}", inventory.is_raspberry_pi());
    println!(
        "hardware_capabilities={}",
        serde_json::to_string(&inventory.capabilities)?
    );
    println!(
        "distributed_node_features={}",
        serde_json::to_string(&node_features)?
    );
    println!("hardware_operations=read_only_typed_probes");
    println!("generic_shell=false");
    println!("raw_device_write=false");
    println!("status=arm_worker_ready");
    Ok(())
}

fn arm_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let report = run_phase20_fixture()?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.arm64_detected
        || !report.raspberry_pi_detected
        || !report.gpio_detected
        || !report.i2c_detected
        || !report.spi_detected
        || !report.uart_detected
        || !report.thermal_detected
        || !report.all_probes_bounded
    {
        return Err("one or more Phase 20 ARM/Raspberry Pi fixtures failed".into());
    }
    println!("status=arm_fixture_passed");
    Ok(())
}

fn arm_probe(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let probe_name = value_after(args, "--probe").ok_or(
        "missing --probe board-model|cpu-temperature|gpio-controllers|i2c-buses|spi-devices|serial-devices",
    )?;
    let probe = match probe_name.as_str() {
        "board-model" => HardwareProbe::BoardModel,
        "cpu-temperature" => HardwareProbe::CpuTemperature,
        "gpio-controllers" => HardwareProbe::GpioControllers,
        "i2c-buses" => HardwareProbe::I2cBuses,
        "spi-devices" => HardwareProbe::SpiDevices,
        "serial-devices" => HardwareProbe::SerialDevices,
        _ => return Err("unsupported ARM hardware probe".into()),
    };
    let inspector = arm_host_inspector()?;
    let results = inspector.run_plan(&HardwarePlan {
        probes: vec![probe],
    })?;
    println!("{}", serde_json::to_string_pretty(&results[0])?);
    println!("status=arm_probe_passed");
    Ok(())
}

fn arm_host_inspector() -> Result<ArmInspector, Box<dyn std::error::Error>> {
    if std::env::consts::OS != "linux" {
        return Err("Phase 20 ARM hardware inspection requires Linux".into());
    }
    let inspector = ArmInspector::host();
    let inventory = inspector.inventory()?;
    if !inventory.is_arm() {
        return Err("Phase 20 ARM hardware inspection requires an ARM/AArch64 host".into());
    }
    Ok(inspector)
}

fn arm_node_features(inventory: &df_test_arm::ArmInventory) -> BTreeSet<NodeFeature> {
    let mut features = BTreeSet::new();
    if !inventory.is_arm() {
        return features;
    }
    features.insert(NodeFeature::ArmWorker);
    if inventory.is_raspberry_pi() {
        features.insert(NodeFeature::RaspberryPi);
    }
    if inventory.capabilities.contains(&ArmCapability::Gpio) {
        features.insert(NodeFeature::HardwareIo);
        features.insert(NodeFeature::Gpio);
    }
    if inventory.capabilities.contains(&ArmCapability::I2c) {
        features.insert(NodeFeature::HardwareIo);
        features.insert(NodeFeature::I2c);
    }
    if inventory.capabilities.contains(&ArmCapability::Spi) {
        features.insert(NodeFeature::HardwareIo);
        features.insert(NodeFeature::Spi);
    }
    if inventory.capabilities.contains(&ArmCapability::Uart) {
        features.insert(NodeFeature::HardwareIo);
        features.insert(NodeFeature::Uart);
    }
    features
}

fn install_doctor() -> Result<(), Box<dyn std::error::Error>> {
    let platform = InstallPlatform::current()?;
    let layout = InstallLayout::production(platform);
    let config = ManagedInstallConfig::for_layout(&layout);
    config.validate()?;

    println!("DragonForge Test Lab installer doctor");
    println!("platform={}", platform.as_str());
    println!("binary_path={}", layout.binary_path.display());
    println!("config_root={}", layout.config_root.display());
    println!("state_root={}", layout.state_root.display());
    println!("log_root={}", layout.log_root.display());
    println!("backup_root={}", layout.backup_root.display());
    println!("service_name={}", layout.service_name);
    println!(
        "manifest_schema={}",
        df_test_install::INSTALL_MANIFEST_VERSION
    );
    println!(
        "install_state_schema={}",
        df_test_install::INSTALL_STATE_VERSION
    );
    println!(
        "install_config_schema={}",
        df_test_install::INSTALL_CONFIG_VERSION
    );
    println!("rollback=previous_version_only");
    println!("uninstall_preserves_state_by_default=true");
    println!("status=installer_ready");
    Ok(())
}

fn install_layout() -> Result<(), Box<dyn std::error::Error>> {
    let layout = InstallLayout::production(InstallPlatform::current()?);
    println!("{}", serde_json::to_string_pretty(&layout)?);
    Ok(())
}

fn release_verify(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let manifest_path =
        value_after(args, "--manifest").ok_or("missing --manifest <release-manifest.json>")?;
    let package_root =
        value_after(args, "--package-root").ok_or("missing --package-root <directory>")?;
    let manifest = ReleaseManifest::load(manifest_path)?;
    let binary =
        manifest.verify_package(package_root, std::env::consts::OS, std::env::consts::ARCH)?;
    println!("version={}", manifest.version);
    println!("target={}/{}", manifest.target_os, manifest.target_arch);
    println!("binary={}", binary.display());
    println!("checksum=verified");
    println!("status=release_verified");
    Ok(())
}

fn upgrade_plan(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let state_path = value_after(args, "--state").ok_or("missing --state <install-state.json>")?;
    let manifest_path =
        value_after(args, "--manifest").ok_or("missing --manifest <release-manifest.json>")?;
    let state = InstallState::load(state_path)?;
    let manifest = ReleaseManifest::load(manifest_path)?;
    let layout = InstallLayout::production(InstallPlatform::current()?);
    let plan = state.plan_upgrade(&manifest, &layout)?;
    println!("{}", serde_json::to_string_pretty(&plan)?);
    println!("status=upgrade_plan_ready");
    Ok(())
}

fn install_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!(
        "dragonforge-phase21-install-fixture-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let layout = InstallLayout::fixture(&root, InstallPlatform::Linux);
    layout.create_data_directories()?;

    let binary = root.join("dragonforge-test-lab");
    std::fs::write(&binary, b"phase21-current-binary")?;
    let current_hash = df_test_install::sha256_file(&binary, 1024 * 1024)?;
    let state = InstallState::fresh("0.21.0", current_hash)?;
    state.save(&layout.install_state_path)?;

    let config_path = layout.config_root.join("install-config.json");
    ManagedInstallConfig::for_layout(&layout).save(&config_path)?;

    let package = root.join("package");
    std::fs::create_dir_all(&package)?;
    let target_binary = package.join("dragonforge-test-lab");
    std::fs::write(&target_binary, b"phase21-target-binary")?;
    let target_hash = df_test_install::sha256_file(&target_binary, 1024 * 1024)?;
    let manifest = ReleaseManifest {
        schema_version: df_test_install::INSTALL_MANIFEST_VERSION,
        version: "0.22.0".into(),
        target_os: "linux".into(),
        target_arch: std::env::consts::ARCH.into(),
        binary_file: "dragonforge-test-lab".into(),
        binary_sha256: target_hash,
    };
    let verified = manifest.verify_package(&package, "linux", std::env::consts::ARCH)?;
    let plan = state.plan_upgrade(&manifest, &layout)?;
    let upgraded = state.upgraded(&manifest)?;
    let rollback = upgraded.rollback_target()?;

    let report = serde_json::json!({
        "layout_created": layout.config_root.is_dir()
            && layout.state_root.is_dir()
            && layout.log_root.is_dir()
            && layout.backup_root.is_dir(),
        "manifest_verified": verified == target_binary,
        "upgrade_from": plan.from_version,
        "upgrade_to": plan.to_version,
        "rollback_version": rollback.version,
        "config_schema": ManagedInstallConfig::load(&config_path)?.schema_version,
        "state_round_trip": InstallState::load(&layout.install_state_path)? == state
    });
    println!("{}", serde_json::to_string_pretty(&report)?);

    let passed = report["layout_created"] == true
        && report["manifest_verified"] == true
        && report["upgrade_from"] == "0.21.0"
        && report["upgrade_to"] == "0.22.0"
        && report["rollback_version"] == "0.21.0"
        && report["config_schema"] == df_test_install::INSTALL_CONFIG_VERSION
        && report["state_round_trip"] == true;
    let _ = std::fs::remove_dir_all(root);
    if !passed {
        return Err("one or more Phase 21 installer fixtures failed".into());
    }
    println!("status=installer_fixture_passed");
    Ok(())
}

fn release_doctor() -> Result<(), Box<dyn std::error::Error>> {
    println!("DragonForge Test Lab release engineering doctor");
    println!("bundle_schema={RELEASE_BUNDLE_SCHEMA_VERSION}");
    println!("channels=dev,beta,stable");
    println!("stable_signatures_required=true");
    println!("checksum=sha256");
    println!("sbom=cyclonedx_json");
    println!("dependency_audit=cargo_audit");
    println!("license_audit=cargo_deny");
    println!("signing_keys_in_repository=false");
    println!("status=release_engineering_ready");
    Ok(())
}

fn release_tag(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let channel = ReleaseChannel::parse(
        &value_after(args, "--channel").ok_or("missing --channel dev|beta|stable")?,
    )?;
    let version = value_after(args, "--version").ok_or("missing --version <version>")?;
    let parsed = ReleaseVersion::parse(&version, channel)?;
    println!("{}", parsed.tag());
    Ok(())
}

fn release_bundle_verify(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let manifest_path =
        value_after(args, "--manifest").ok_or("missing --manifest <release-bundle.json>")?;
    let root = value_after(args, "--root").ok_or("missing --root <release-directory>")?;
    let bytes = std::fs::read(manifest_path)?;
    if bytes.len() > 1024 * 1024 {
        return Err("release bundle manifest exceeds 1 MiB".into());
    }
    let manifest: ReleaseBundleManifest =
        serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes))?;
    manifest.verify_root(root)?;
    println!("version={}", manifest.version);
    println!("channel={}", manifest.channel.as_str());
    println!("artifacts={}", manifest.artifacts.len());
    println!("status=release_bundle_verified");
    Ok(())
}

fn release_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!(
        "dragonforge-phase22-release-fixture-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root)?;

    let package = root.join("dragonforge-test-lab-0.23.0-linux-x86_64.tar.gz");
    let signature = root.join("dragonforge-test-lab-0.23.0-linux-x86_64.tar.gz.sig");
    let sbom = root.join("dragonforge-test-lab-0.23.0.cdx.json");
    let checksums = root.join("SHA256SUMS");
    let audit = root.join("audit-report.txt");
    let notes = root.join("RELEASE-NOTES.md");

    std::fs::write(&package, b"phase22-package")?;
    std::fs::write(&signature, b"phase22-signature")?;
    std::fs::write(
        &sbom,
        b"{\"bomFormat\":\"CycloneDX\",\"specVersion\":\"1.6\"}",
    )?;
    std::fs::write(&checksums, b"fixture checksum index")?;
    std::fs::write(&audit, b"audit fixture passed")?;
    std::fs::write(&notes, b"# Phase 22 fixture")?;

    let manifest = ReleaseBundleManifest {
        schema_version: RELEASE_BUNDLE_SCHEMA_VERSION,
        version: "0.23.0".into(),
        channel: ReleaseChannel::Stable,
        git_commit: "d".repeat(40),
        artifacts: vec![
            ReleaseArtifact {
                kind: ReleaseArtifactKind::LinuxPackage,
                file: package.file_name().unwrap().to_string_lossy().into_owned(),
                sha256: df_test_release::sha256_file(&package)?,
                signature_file: Some(
                    signature
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                ),
            },
            ReleaseArtifact {
                kind: ReleaseArtifactKind::Sbom,
                file: sbom.file_name().unwrap().to_string_lossy().into_owned(),
                sha256: df_test_release::sha256_file(&sbom)?,
                signature_file: None,
            },
            ReleaseArtifact {
                kind: ReleaseArtifactKind::Checksums,
                file: checksums
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                sha256: df_test_release::sha256_file(&checksums)?,
                signature_file: None,
            },
            ReleaseArtifact {
                kind: ReleaseArtifactKind::AuditReport,
                file: audit.file_name().unwrap().to_string_lossy().into_owned(),
                sha256: df_test_release::sha256_file(&audit)?,
                signature_file: None,
            },
            ReleaseArtifact {
                kind: ReleaseArtifactKind::ReleaseNotes,
                file: notes.file_name().unwrap().to_string_lossy().into_owned(),
                sha256: df_test_release::sha256_file(&notes)?,
                signature_file: None,
            },
        ],
    };
    manifest.verify_root(&root)?;
    let beta = ReleaseVersion::parse("0.23.0-beta.1", ReleaseChannel::Beta)?;
    if beta.tag() != "v0.23.0-beta.1" {
        return Err("beta release tag fixture failed".into());
    }
    println!("{}", serde_json::to_string_pretty(&manifest)?);
    println!("status=release_fixture_passed");
    let _ = std::fs::remove_dir_all(root);
    Ok(())
}

fn security_doctor() -> Result<(), Box<dyn std::error::Error>> {
    println!("DragonForge Test Lab security review doctor");
    println!("review_schema=1");
    println!(
        "review_scope=protocol,policy,workers,paths,artifacts,mcp,certificates,transport,dos,secrets,logs,persistence,privileges,installers,releases"
    );
    println!("blocking_severity=high,critical");
    println!("generic_shell=false");
    println!("arbitrary_file_read=false");
    println!("status=security_review_ready");
    Ok(())
}

fn security_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let report = run_security_fixture()?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.passed() {
        return Err("Phase 23 security fixture reported a blocking finding".into());
    }
    println!("status=security_fixture_passed");
    Ok(())
}

fn security_review(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let root = value_after(args, "--root")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let report = run_security_review(&root)?;
    let json = serde_json::to_string_pretty(&report)?;
    if let Some(output) = value_after(args, "--output") {
        let output = PathBuf::from(output);
        if let Some(parent) = output.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(&output, &json)?;
        println!("report={}", output.display());
    }
    println!("{json}");
    println!("blocking_findings={}", report.blocking_findings());
    if !report.passed() {
        return Err(
            "security review failed closed because one or more required invariants were missing or blocking findings were detected"
                .into(),
        );
    }
    println!("status=security_review_passed");
    Ok(())
}

fn chaos_doctor() -> Result<(), Box<dyn std::error::Error>> {
    println!("DragonForge Test Lab reliability / chaos doctor");
    println!("chaos_schema=1");
    println!("controller_restart=true");
    println!("duplicate_assignment_guard=true");
    println!("stale_node_lease=true");
    println!("replay_rejection=true");
    println!("worker_restart_recovery=true");
    println!("bounded_reconnect_backoff=true");
    println!("database_corruption_fail_closed=true");
    println!("disk_write_failure_fail_closed=true");
    println!("certificate_revocation_fail_closed=true");
    println!("default_stress_jobs={DEFAULT_STRESS_JOBS}");
    println!("max_stress_jobs={MAX_STRESS_JOBS}");
    println!("status=chaos_ready");
    Ok(())
}

fn chaos_fixture(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let stress_jobs = match value_after(args, "--stress-jobs") {
        Some(value) => value.parse::<usize>()?,
        None => DEFAULT_STRESS_JOBS,
    };
    let report = run_chaos_fixture(stress_jobs)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.passed() {
        return Err("one or more Phase 24 reliability / chaos scenarios failed".into());
    }
    println!("status=chaos_fixture_passed");
    Ok(())
}

fn dogfood_doctor() -> Result<(), Box<dyn std::error::Error>> {
    println!("DragonForge Test Lab dogfooding doctor");
    println!("dogfood_schema={DOGFOOD_SCHEMA_VERSION}");
    println!("immutable_revisions=true");
    println!("typed_actions_only=true");
    println!("self_host_depth=1");
    println!("auto_merge=false");
    println!("generic_shell=false");
    println!("status=dogfood_ready");
    Ok(())
}

fn read_dogfood_bytes(path: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() == 0 || metadata.len() > MAX_PROFILE_BYTES as u64 {
        return Err("dogfood input must be between 1 byte and 1 MiB".into());
    }
    Ok(std::fs::read(path)?)
}

fn read_dogfood_profile(path: &str) -> Result<DogfoodProfile, Box<dyn std::error::Error>> {
    Ok(load_profile(&read_dogfood_bytes(path)?)?)
}

fn dogfood_profile_validate(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = value_after(args, "--profile").ok_or("missing --profile <dogfood-profile.json>")?;
    let profile = read_dogfood_profile(&path)?;
    println!("{}", serde_json::to_string_pretty(&profile)?);
    println!("status=dogfood_profile_valid");
    Ok(())
}

fn dogfood_profile_compile(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = value_after(args, "--profile").ok_or("missing --profile <dogfood-profile.json>")?;
    let sha = value_after(args, "--sha").ok_or("missing --sha <immutable-commit>")?;
    let depth = value_after(args, "--depth")
        .unwrap_or_else(|| "0".into())
        .parse::<u8>()?;
    let profile = read_dogfood_profile(&path)?;
    let job = profile.compile_job(&sha, depth)?;
    println!("{}", serde_json::to_string_pretty(&job)?);
    println!("status=dogfood_profile_compiled");
    Ok(())
}

fn dogfood_campaign_validate(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = value_after(args, "--campaign").ok_or("missing --campaign <campaign.json>")?;
    let bytes = read_dogfood_bytes(&path)?;
    let campaign = load_campaign(&bytes)?;
    println!("{}", serde_json::to_string_pretty(&campaign)?);
    println!("enabled_profiles={}", campaign.enabled_profiles().count());
    println!("status=dogfood_campaign_valid");
    Ok(())
}

fn dogfood_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let self_hosted: DogfoodProfile = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "name": "dragonforge-test-lab",
        "repository": "https://github.com/djames1987/DragonForge-Test-Lab.git",
        "default_revision": "main",
        "validation_profile": "rust_standard",
        "enabled": true,
        "self_hosted": true,
        "max_depth": 1,
        "manual_validation": ["physical ARM qualification remains manual"],
        "limits": {
            "timeout_seconds": 1800,
            "max_memory_mib": 8192,
            "max_disk_mib": 16384,
            "max_processes": 128
        }
    }))?;
    self_hosted.validate()?;
    let sha = "a".repeat(40);
    let job = self_hosted.compile_job(&sha, 1)?;
    if job.repository.revision != sha
        || !matches!(job.actions.first(), Some(TestAction::Checkout))
        || self_hosted.compile_job(&"b".repeat(40), 0).is_ok()
        || self_hosted.compile_job(&"c".repeat(40), 2).is_ok()
    {
        return Err("Phase 25 dogfood recursion/immutable-SHA fixture failed".into());
    }
    println!("immutable_sha_compilation=passed");
    println!("self_host_recursion_guard=passed");
    println!("typed_action_surface=passed");
    println!("status=dogfood_fixture_passed");
    Ok(())
}

fn dogfood_campaign_run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = value_after(args, "--campaign").ok_or("missing --campaign <campaign.json>")?;
    let bytes = std::fs::read(path)?;
    let campaign = load_campaign(&bytes)?;
    let github = GhGitHubClient::default();
    github.doctor()?;
    let mut runs = Vec::new();

    for profile in campaign.enabled_profiles() {
        let repository = GitHubRepository::parse_https(&profile.repository)?;
        let immutable_sha = github.resolve_commit(&repository, &profile.default_revision)?;
        let depth = if profile.self_hosted { 1 } else { 0 };
        let job = profile.compile_job(&immutable_sha, depth)?;
        println!(
            "dogfood_campaign_profile={} repository={} resolved_commit={}",
            profile.name,
            repository.slug(),
            immutable_sha
        );

        let report = execute_typed_job(
            job,
            lab_root(args).join(&profile.name),
            args.iter().any(|arg| arg == "--retain-workspace"),
            sandbox_mode(args)?,
            value_after(args, "--worker-user"),
        )?;
        let status = job_status_name(report.status).to_owned();
        runs.push(DogfoodRunRecord {
            profile: profile.name.clone(),
            repository: profile.repository.clone(),
            immutable_sha,
            status,
            summary: report.summary.clone(),
            artifact_directory: report.artifact_directory.clone(),
        });
        if report.status != JobStatus::Passed {
            break;
        }
    }

    let report = DogfoodCampaignReport {
        schema_version: DOGFOOD_SCHEMA_VERSION,
        campaign: campaign.name,
        runs,
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.passed() {
        std::process::exit(2);
    }
    println!("status=dogfood_campaign_passed");
    Ok(())
}

fn job_status_name(status: JobStatus) -> &'static str {
    match status {
        JobStatus::Queued => "queued",
        JobStatus::Assigned => "assigned",
        JobStatus::Running => "running",
        JobStatus::Passed => "passed",
        JobStatus::Failed => "failed",
        JobStatus::Rejected => "rejected",
        JobStatus::Cancelled => "cancelled",
    }
}

fn dogfood_run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = value_after(args, "--profile").ok_or("missing --profile <dogfood-profile.json>")?;
    let profile = read_dogfood_profile(&path)?;
    if !profile.enabled {
        return Err("dogfood profile is disabled".into());
    }
    let requested_revision =
        value_after(args, "--revision").unwrap_or_else(|| profile.default_revision.clone());
    let depth = value_after(args, "--depth")
        .unwrap_or_else(|| if profile.self_hosted { "1".into() } else { "0".into() })
        .parse::<u8>()?;

    let repository = GitHubRepository::parse_https(&profile.repository)?;
    let github = GhGitHubClient::default();
    github.doctor()?;
    let immutable_sha = github.resolve_commit(&repository, &requested_revision)?;
    let job = profile.compile_job(&immutable_sha, depth)?;

    println!("dogfood_profile={}", profile.name);
    println!("github_repository={}", repository.slug());
    println!("requested_revision={requested_revision}");
    println!("resolved_commit={immutable_sha}");
    println!("dogfood_depth={depth}");

    let report = execute_typed_job(
        job,
        lab_root(args),
        args.iter().any(|arg| arg == "--retain-workspace"),
        sandbox_mode(args)?,
        value_after(args, "--worker-user"),
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report.status != JobStatus::Passed {
        std::process::exit(2);
    }
    println!("status=dogfood_run_passed");
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

fn identity_doctor() -> Result<(), Box<dyn std::error::Error>> {
    let loopback: std::net::SocketAddr = "127.0.0.1:443".parse()?;
    let private: std::net::SocketAddr = "10.0.0.1:443".parse()?;
    let public: std::net::SocketAddr = "8.8.8.8:443".parse()?;

    validate_private_controller_address(loopback)?;
    validate_private_controller_address(private)?;
    if validate_private_controller_address(public).is_ok() {
        return Err(
            "mTLS identity controller address policy unexpectedly allowed a public IP".into(),
        );
    }

    println!("DragonForge Test Lab identity doctor");
    println!("transport=mutual_tls");
    println!("tls_library=rustls");
    println!("certificate_enrollment=enabled");
    println!("certificate_renewal=enabled");
    println!("certificate_revocation=enabled");
    println!("identity_binding=sha256_certificate_fingerprint");
    println!("controller_address_policy=loopback_private_link_local");
    println!("legacy_hmac_transport=compatibility_only");
    println!("status=mtls_identity_ready");
    Ok(())
}

fn identity_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let report = run_mtls_fixture()?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.encrypted_round_trip
        || !report.client_certificate_observed
        || !report.server_certificate_observed
        || !report.node_identity_verified
        || !report.renewal_verified
        || !report.revocation_verified
    {
        return Err("Phase 12 mTLS identity fixture failed".into());
    }
    println!("status=mtls_identity_fixture_passed");
    Ok(())
}

fn lifecycle_doctor() -> Result<(), Box<dyn std::error::Error>> {
    let policy = RetryPolicy::bounded(3, 5, 60, true)?;
    if policy.max_attempts != 3
        || policy.base_delay_secs != 5
        || policy.max_delay_secs != 60
        || !policy.retry_interrupted
    {
        return Err("retry policy validation produced unexpected values".into());
    }

    let controller = DurableController::open_in_memory()?;
    if controller.schema_version()? < 3 {
        return Err("controller lifecycle schema is unavailable".into());
    }

    println!("DragonForge Test Lab lifecycle doctor");
    println!("controller_schema={}", controller.schema_version()?);
    println!("failure_classification=explicit");
    println!("test_failure_retry=disabled");
    println!("transient_infrastructure_retry=bounded");
    println!("interrupted_retry=explicit_opt_in");
    println!("max_attempts={}", df_test_lifecycle::MAX_ATTEMPTS);
    println!("retry_backoff=bounded_exponential");
    println!("manual_interrupted_reschedule=enabled");
    println!("status=lifecycle_ready");
    Ok(())
}

fn lifecycle_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let mut controller = DurableController::open_in_memory()?;
    let worker = WorkerRegistration {
        worker_id: "phase15-fixture-worker".into(),
        protocol_version: PROTOCOL_VERSION,
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        capabilities: [Capability::CheckoutRepository, Capability::CargoTest]
            .into_iter()
            .collect(),
    };
    controller.register_worker(&worker, 1)?;

    let retry_job = JobRequest::new(
        RepositorySpec {
            url: "https://github.com/djames1987/DragonForge-Test-Lab.git".into(),
            revision: "main".into(),
        },
        vec![
            TestAction::Checkout,
            TestAction::CargoTest { all_features: true },
        ],
    );
    controller.enqueue_job_with_retry(&retry_job, RetryPolicy::bounded(3, 5, 30, false)?, 2)?;
    controller
        .assign_next(&worker.worker_id, 3)?
        .ok_or("Phase 15 fixture did not assign initial retry job")?;
    controller.mark_running(retry_job.id, 4)?;

    let retry_decision = controller.complete_job_with_classification(
        &df_test_protocol::JobResult {
            job_id: retry_job.id,
            status: JobStatus::Failed,
            summary: "fixture transport interruption".into(),
            artifacts: vec![],
        },
        Some(FailureClass::InfrastructureTransient),
        5,
    )?;
    let transient_retry_scheduled = retry_decision
        == LifecycleDecision::RetryScheduled {
            next_retry_at_secs: 10,
        };
    let retry_not_early = controller.assign_next(&worker.worker_id, 9)?.is_none();
    let retry_assigned_when_due = controller
        .assign_next(&worker.worker_id, 10)?
        .map(|job| job.id)
        == Some(retry_job.id);
    controller.mark_running(retry_job.id, 11)?;
    let test_failure_decision = controller.complete_job_with_classification(
        &df_test_protocol::JobResult {
            job_id: retry_job.id,
            status: JobStatus::Failed,
            summary: "fixture assertion failure".into(),
            artifacts: vec![],
        },
        Some(FailureClass::TestFailure),
        12,
    )?;
    let test_failure_terminal = test_failure_decision == LifecycleDecision::TerminalFailed
        && controller.get_job(retry_job.id)?.map(|job| job.state) == Some(DurableJobState::Failed);
    let attempt_history_preserved = controller.list_attempts(retry_job.id)?.len() == 2;

    let recovery_job = JobRequest::new(
        RepositorySpec {
            url: "https://github.com/djames1987/DragonForge-Test-Lab.git".into(),
            revision: "main".into(),
        },
        vec![TestAction::Checkout],
    );
    controller.enqueue_job_with_retry(&recovery_job, RetryPolicy::bounded(2, 7, 30, true)?, 20)?;
    controller
        .assign_next(&worker.worker_id, 21)?
        .ok_or("Phase 15 fixture did not assign recovery job")?;
    controller.mark_running(recovery_job.id, 22)?;
    controller.recover_after_restart(30)?;
    let recovered = controller
        .get_job(recovery_job.id)?
        .ok_or("Phase 15 recovery job disappeared")?;
    let interrupted_retry_scheduled = recovered.state == DurableJobState::RetryPending
        && recovered.failure_class == Some(FailureClass::Interrupted)
        && recovered.next_retry_at_secs == Some(37);

    let manual_job = JobRequest::new(
        RepositorySpec {
            url: "https://github.com/djames1987/DragonForge-Test-Lab.git".into(),
            revision: "main".into(),
        },
        vec![TestAction::Checkout],
    );
    controller.enqueue_job(&manual_job, 31)?;
    controller
        .assign_next(&worker.worker_id, 32)?
        .ok_or("Phase 15 fixture did not assign manual recovery job")?;
    controller.mark_running(manual_job.id, 33)?;
    controller.recover_after_restart(34)?;
    let manual_interrupted = controller.get_job(manual_job.id)?.map(|job| job.state)
        == Some(DurableJobState::Interrupted);
    controller.reschedule_interrupted_job(manual_job.id, 35)?;
    let manual_interrupted_reschedule = manual_interrupted
        && controller.get_job(manual_job.id)?.map(|job| job.state) == Some(DurableJobState::Queued);

    let audit_chain_verified = controller.verify_audit_chain()?;
    let schema_v3 = controller.schema_version()? >= 3;

    let report = serde_json::json!({
        "schema_v3": schema_v3,
        "transient_retry_scheduled": transient_retry_scheduled,
        "retry_not_early": retry_not_early,
        "retry_assigned_when_due": retry_assigned_when_due,
        "test_failure_terminal": test_failure_terminal,
        "interrupted_retry_scheduled": interrupted_retry_scheduled,
        "manual_interrupted_reschedule": manual_interrupted_reschedule,
        "attempt_history_preserved": attempt_history_preserved,
        "audit_chain_verified": audit_chain_verified
    });
    println!("{}", serde_json::to_string_pretty(&report)?);

    if !schema_v3
        || !transient_retry_scheduled
        || !retry_not_early
        || !retry_assigned_when_due
        || !test_failure_terminal
        || !interrupted_retry_scheduled
        || !manual_interrupted_reschedule
        || !attempt_history_preserved
        || !audit_chain_verified
    {
        return Err("one or more Phase 15 lifecycle fixtures failed".into());
    }

    println!("status=lifecycle_fixture_passed");
    Ok(())
}

fn lifecycle_status(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = value_after(args, "--state-db")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".dragonforge-test-lab").join("controller.sqlite3"));
    let job_id =
        uuid::Uuid::parse_str(&value_after(args, "--job-id").ok_or("missing --job-id <uuid>")?)?;
    let controller = DurableController::open(&path)?;
    let job = controller
        .get_job(job_id)?
        .ok_or("job was not found in durable controller state")?;
    let attempts = controller.list_attempts(job_id)?;
    println!("DragonForge Test Lab lifecycle status");
    println!("database={}", path.display());
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "job": job,
            "attempts": attempts
        }))?
    );
    println!("status=lifecycle_status_ready");
    Ok(())
}

fn lifecycle_reschedule(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = value_after(args, "--state-db")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".dragonforge-test-lab").join("controller.sqlite3"));
    let job_id =
        uuid::Uuid::parse_str(&value_after(args, "--job-id").ok_or("missing --job-id <uuid>")?)?;
    let mut controller = DurableController::open(&path)?;
    controller.reschedule_interrupted_job(job_id, unix_time_secs()?)?;
    println!("job_id={job_id}");
    println!("state=queued");
    println!("status=lifecycle_rescheduled");
    Ok(())
}

fn plan_doctor() -> Result<(), Box<dyn std::error::Error>> {
    let controller = DurableController::open_in_memory()?;
    if controller.schema_version()? != 4 {
        return Err("controller test-plan schema is not current".into());
    }

    let plan = phase16_fixture_plan();
    plan.validate()?;
    let order = plan.topological_order()?;
    if order != vec!["fast".to_owned(), "standard".to_owned()] {
        return Err("test-plan dependency ordering is not deterministic".into());
    }

    println!("DragonForge Test Lab plan doctor");
    println!("plan_version={TEST_PLAN_VERSION}");
    println!("controller_schema=4");
    println!("typed_profiles=enabled");
    println!("typed_actions=enabled");
    println!("dependencies=dag_validated");
    println!("conditions=enabled");
    println!("retry_policy=phase15");
    println!("target_os=enabled");
    println!("node_labels=enabled");
    println!("status=test_plans_ready");
    Ok(())
}

fn plan_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let plan = phase16_fixture_plan();
    plan.validate()?;
    let dependency_order_valid =
        plan.topological_order()? == vec!["fast".to_owned(), "standard".to_owned()];
    let initial_ready = plan.ready_steps(&std::collections::BTreeMap::new())?;
    let initial_ready_valid = initial_ready == vec!["fast".to_owned()];

    let mut completed = std::collections::BTreeMap::new();
    completed.insert("fast".to_owned(), PlanStepStatus::Passed);
    let dependent_ready = plan.ready_steps(&completed)?;
    let dependency_condition_valid = dependent_ready == vec!["standard".to_owned()];

    let fast = plan.compile_step("fast")?;
    let standard = plan.compile_step("standard")?;
    let typed_actions_only = !fast.job.actions.is_empty() && !standard.job.actions.is_empty();
    let target_constraints_valid = standard.matches_target(
        "windows",
        &std::collections::BTreeMap::from([("tier".to_owned(), "primary".to_owned())]),
    ) && !standard.matches_target(
        "linux",
        &std::collections::BTreeMap::from([("tier".to_owned(), "primary".to_owned())]),
    );

    let mut controller = DurableController::open_in_memory()?;
    controller.upsert_test_plan(&plan, 100)?;
    let plan_persisted = controller.get_test_plan(&plan.name)? == Some(plan.clone())
        && controller.list_test_plans()? == vec![plan.name.clone()];

    controller.enqueue_job_with_retry(&fast.job, fast.retry, 101)?;

    let incompatible = WorkerRegistration {
        worker_id: "phase16-incompatible".into(),
        protocol_version: PROTOCOL_VERSION,
        os: "windows".into(),
        arch: std::env::consts::ARCH.into(),
        capabilities: [
            Capability::CheckoutRepository,
            Capability::CargoFmtCheck,
            Capability::CargoTest,
        ]
        .into_iter()
        .collect(),
    };
    controller.register_worker(&incompatible, 102)?;
    let extra_capability_enforced = controller
        .assign_next(&incompatible.worker_id, 103)?
        .is_none();

    let compatible = WorkerRegistration {
        worker_id: "phase16-compatible".into(),
        protocol_version: PROTOCOL_VERSION,
        os: "windows".into(),
        arch: std::env::consts::ARCH.into(),
        capabilities: fast.job.required_capabilities(),
    };
    controller.register_worker(&compatible, 104)?;
    let compiled_job_schedulable = controller
        .assign_next(&compatible.worker_id, 105)?
        .map(|job| job.id)
        == Some(fast.job.id);

    let plan_audit = controller.audit_events_for("test_plan", &plan.name)?;
    let plan_audited = plan_audit.len() == 1 && plan_audit[0].kind == "test_plan_created";
    let audit_chain_verified = controller.verify_audit_chain()?;
    let schema_supports_plans = controller.schema_version()? >= 4;

    let report = serde_json::json!({
        "schema_supports_plans": schema_supports_plans,
        "dependency_order_valid": dependency_order_valid,
        "initial_ready_valid": initial_ready_valid,
        "dependency_condition_valid": dependency_condition_valid,
        "typed_actions_only": typed_actions_only,
        "target_constraints_valid": target_constraints_valid,
        "plan_persisted": plan_persisted,
        "extra_capability_enforced": extra_capability_enforced,
        "compiled_job_schedulable": compiled_job_schedulable,
        "plan_audited": plan_audited,
        "audit_chain_verified": audit_chain_verified
    });
    println!("{}", serde_json::to_string_pretty(&report)?);

    if !schema_supports_plans
        || !dependency_order_valid
        || !initial_ready_valid
        || !dependency_condition_valid
        || !typed_actions_only
        || !target_constraints_valid
        || !plan_persisted
        || !extra_capability_enforced
        || !compiled_job_schedulable
        || !plan_audited
        || !audit_chain_verified
    {
        return Err("one or more Phase 16 test-plan fixtures failed".into());
    }

    println!("status=test_plan_fixture_passed");
    Ok(())
}

fn phase16_fixture_plan() -> TestPlan {
    TestPlan {
        version: TEST_PLAN_VERSION,
        name: "phase16-core".into(),
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
                required_capabilities: [
                    Capability::CheckoutRepository,
                    Capability::CargoFmtCheck,
                    Capability::CargoTest,
                    Capability::ReadArtifacts,
                ]
                .into_iter()
                .collect(),
                artifacts: vec![ArtifactKind::StepLogs],
                retry: RetryPolicy::no_retry(),
                target_os: TargetOs::Any,
                node_labels: Default::default(),
            },
            PlanStep {
                id: "standard".into(),
                profile: PlanProfile::RustStandard,
                depends_on: vec!["fast".into()],
                condition: PlanCondition::DependenciesPassed,
                limits: ResourceLimits::default(),
                required_capabilities: [
                    Capability::CheckoutRepository,
                    Capability::CargoFmtCheck,
                    Capability::CargoClippy,
                    Capability::CargoTest,
                ]
                .into_iter()
                .collect(),
                artifacts: vec![ArtifactKind::ExecutionReport],
                retry: RetryPolicy::bounded(2, 5, 30, false)
                    .expect("fixture retry policy is valid"),
                target_os: TargetOs::Windows,
                node_labels: std::collections::BTreeMap::from([("tier".into(), "primary".into())]),
            },
        ],
    }
}

fn load_test_plan(path: &std::path::Path) -> Result<TestPlan, Box<dyn std::error::Error>> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err("test plan must be a regular JSON file no larger than 1 MiB".into());
    }
    let plan: TestPlan = serde_json::from_slice(&std::fs::read(path)?)?;
    plan.validate()?;
    Ok(plan)
}

fn plan_validate(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(value_after(args, "--plan").ok_or("missing --plan <plan.json>")?);
    let plan = load_test_plan(&path)?;
    println!("plan_name={}", plan.name);
    println!("plan_version={}", plan.version);
    println!("step_count={}", plan.steps.len());
    println!("order={}", plan.topological_order()?.join(","));
    println!("status=test_plan_valid");
    Ok(())
}

fn plan_compile(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(value_after(args, "--plan").ok_or("missing --plan <plan.json>")?);
    let step_id = value_after(args, "--step").ok_or("missing --step <id>")?;
    let plan = load_test_plan(&path)?;
    let compiled = plan.compile_step(&step_id)?;
    println!("{}", serde_json::to_string_pretty(&compiled)?);
    println!("status=test_plan_step_compiled");
    Ok(())
}

fn plan_store(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(value_after(args, "--plan").ok_or("missing --plan <plan.json>")?);
    let state_db = value_after(args, "--state-db")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".dragonforge-test-lab").join("controller.sqlite3"));
    if let Some(parent) = state_db.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let plan = load_test_plan(&path)?;
    let mut controller = DurableController::open(&state_db)?;
    controller.upsert_test_plan(&plan, unix_time_secs()?)?;
    println!("plan_name={}", plan.name);
    println!("database={}", state_db.display());
    println!("status=test_plan_stored");
    Ok(())
}

fn plan_list(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let state_db = value_after(args, "--state-db")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".dragonforge-test-lab").join("controller.sqlite3"));
    let controller = DurableController::open(&state_db)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&controller.list_test_plans()?)?
    );
    println!("status=test_plan_list_ready");
    Ok(())
}

fn observability_doctor() -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = MetricsRegistry::default();
    registry.increment("dragonforge_jobs_total", 1)?;
    registry.set_gauge("dragonforge_workers_online", 1.0)?;
    let snapshot = registry.snapshot(unix_time_secs()?);
    if snapshot.len() != 2 {
        return Err("metrics registry did not produce expected snapshot".into());
    }

    let mut controller = DurableController::open_in_memory()?;
    if controller.schema_version()? != SCHEMA_VERSION {
        return Err("controller observability schema is not current".into());
    }
    controller.record_metric(&snapshot[0])?;
    if controller.recent_metrics(10)?.is_empty() {
        return Err("durable metric sample was not persisted".into());
    }

    println!("DragonForge Test Lab observability doctor");
    println!("controller_schema={SCHEMA_VERSION}");
    println!("audit_hash_chain=enabled");
    println!("structured_logs=durable_and_jsonl");
    println!("log_secret_redaction=enabled");
    println!("metrics=durable");
    println!("artifact_sha256_catalog=enabled");
    println!("artifact_retention=root_contained");
    println!("telemetry_pruning=enabled");
    println!("status=observability_ready");
    Ok(())
}

fn observability_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!(
        "dragonforge-phase14-fixture-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root)?;
    let log_path = root.join("logs").join("controller.jsonl");
    let artifact_root = root.join("artifacts");
    std::fs::create_dir_all(&artifact_root)?;

    let mut fields = std::collections::BTreeMap::new();
    fields.insert(
        "api_token".to_owned(),
        serde_json::Value::String("phase14-secret-must-not-persist".to_owned()),
    );
    let log_event = StructuredLogEvent {
        unix_time_secs: 1_000,
        level: LogLevel::Info,
        component: "controller".into(),
        message: "phase14 fixture connected worker".into(),
        fields,
        job_id: None,
        worker_id: Some("phase14-worker".into()),
    };

    let log_writer = JsonlLogWriter::new(&log_path, 4096)?;
    log_writer.append(log_event.clone())?;
    let log_content = std::fs::read_to_string(&log_path)?;
    let jsonl_redacted = log_content.contains("<redacted>")
        && !log_content.contains("phase14-secret-must-not-persist");

    let old_path = artifact_root.join("old.log");
    let current_path = artifact_root.join("current.log");
    std::fs::write(&old_path, b"old-artifact")?;
    std::fs::write(&current_path, b"current-artifact")?;
    let old = catalog_artifact(&artifact_root, &old_path, 100)?;
    let current = catalog_artifact(&artifact_root, &current_path, 950)?;
    let sha256_cataloged = old.sha256.len() == 64 && current.sha256.len() == 64;
    let prune = prune_artifacts(
        &artifact_root,
        &[old.clone(), current.clone()],
        ArtifactRetentionPolicy {
            max_age_secs: 500,
            max_total_bytes: 1024 * 1024,
            max_artifacts: 10,
        },
        1_000,
    )?;
    let artifact_retention_verified = prune.removed == vec![old.relative_path.clone()]
        && current_path.exists()
        && !old_path.exists();

    let mut controller = DurableController::open_in_memory()?;
    let job = JobRequest::new(
        RepositorySpec {
            url: "https://github.com/djames1987/DragonForge-Test-Lab.git".into(),
            revision: "main".into(),
        },
        vec![TestAction::Checkout],
    );
    controller.enqueue_job(&job, 1_001)?;
    controller.record_log(&log_event)?;

    let metric = MetricPoint {
        unix_time_secs: 1_002,
        name: "dragonforge_workers_online".into(),
        value: 1.0,
        labels: Default::default(),
    };
    controller.record_metric(&metric)?;

    let durable_log = controller.recent_logs(10)?;
    let durable_log_redacted = durable_log
        .first()
        .and_then(|record| record.event.fields.get("api_token"))
        == Some(&serde_json::Value::String("<redacted>".into()));
    let durable_metric_persisted = controller
        .recent_metrics(10)?
        .first()
        .map(|record| record.point.name.as_str())
        == Some("dragonforge_workers_online");
    let audit_chain_verified = controller.verify_audit_chain()?;
    let schema_v2 = controller.schema_version()? >= 2;
    let telemetry_pruned = controller.prune_telemetry_before(2_000)? == (1, 1);

    let report = serde_json::json!({
        "schema_v2": schema_v2,
        "audit_chain_verified": audit_chain_verified,
        "jsonl_redacted": jsonl_redacted,
        "durable_log_redacted": durable_log_redacted,
        "durable_metric_persisted": durable_metric_persisted,
        "sha256_cataloged": sha256_cataloged,
        "artifact_retention_verified": artifact_retention_verified,
        "telemetry_pruned": telemetry_pruned
    });
    println!("{}", serde_json::to_string_pretty(&report)?);

    if !schema_v2
        || !audit_chain_verified
        || !jsonl_redacted
        || !durable_log_redacted
        || !durable_metric_persisted
        || !sha256_cataloged
        || !artifact_retention_verified
        || !telemetry_pruned
    {
        return Err("one or more Phase 14 observability fixtures failed".into());
    }

    println!("status=observability_fixture_passed");
    let _ = std::fs::remove_dir_all(root);
    Ok(())
}

fn observability_summary(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = value_after(args, "--state-db")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".dragonforge-test-lab").join("controller.sqlite3"));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let controller = DurableController::open(&path)?;
    println!("DragonForge Test Lab observability summary");
    println!("database={}", path.display());
    println!("schema_version={}", controller.schema_version()?);
    println!("audit_chain_valid={}", controller.verify_audit_chain()?);
    println!("recent_logs={}", controller.recent_logs(100)?.len());
    println!("recent_metrics={}", controller.recent_metrics(100)?.len());
    println!("status=observability_summary_ready");
    Ok(())
}

fn worker_service_doctor() -> Result<(), Box<dyn std::error::Error>> {
    let report = run_worker_service_fixture()?;
    if !report.mtls_registration
        || !report.heartbeat_received
        || !report.drain_blocks_new_jobs
        || !report.restart_state_recovered
        || !report.windows_service_spec_valid
        || !report.systemd_unit_valid
    {
        return Err("worker service fixture failed during doctor".into());
    }
    println!("DragonForge Test Lab worker service doctor");
    println!("transport=outbound_mutual_tls");
    println!("heartbeat=typed");
    println!("drain=graceful");
    println!("restart_state=persistent_non_secret_snapshot");
    println!("windows_service_spec=enabled");
    println!("systemd_unit=enabled");
    println!("status=worker_service_ready");
    Ok(())
}

fn worker_service_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let report = run_worker_service_fixture()?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.mtls_registration
        || !report.heartbeat_received
        || !report.drain_blocks_new_jobs
        || !report.restart_state_recovered
        || !report.windows_service_spec_valid
        || !report.systemd_unit_valid
    {
        return Err("one or more worker service fixtures failed".into());
    }
    println!("status=worker_service_fixture_passed");
    Ok(())
}

fn worker_service_run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    worker_service_run_loop(args, None)
}

fn worker_service_run_loop(
    args: &[String],
    stop_requested: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = value_after(args, "--config").ok_or("missing --config <worker.json>")?;
    let once = args.iter().any(|arg| arg == "--once");
    let config = load_worker_config(&config_path)?;
    let client_config = config.load_client_config()?;
    let mut runtime = WorkerServiceRuntime::recover(config.clone())?;

    loop {
        if service_stop_requested(stop_requested.as_ref()) {
            runtime.request_drain();
            runtime.persist()?;
            return Ok(());
        }

        runtime.mark_connecting();
        runtime.persist()?;
        match MtlsWorkerSession::connect(
            &config,
            client_config.clone(),
            std::time::Duration::from_secs(10),
        ) {
            Ok(mut session) => match session.register(&runtime) {
                Ok(_) => {
                    let now = unix_time_secs()?;
                    runtime.mark_connected(now);
                    runtime.persist()?;
                    session.send_heartbeat(&runtime)?;
                    runtime.mark_heartbeat_sent(now);
                    runtime.persist()?;
                    println!("worker_id={}", config.worker_id);
                    println!("controller={}", config.controller);
                    println!("state={:?}", runtime.snapshot().state);
                    println!("accepting_jobs={}", runtime.can_accept_job());
                    println!("status=worker_service_online");
                    if once {
                        return Ok(());
                    }

                    loop {
                        if interruptible_sleep(
                            std::time::Duration::from_secs(config.heartbeat_seconds),
                            stop_requested.as_ref(),
                        ) {
                            runtime.request_drain();
                            runtime.persist()?;
                            return Ok(());
                        }
                        let now = unix_time_secs()?;
                        if runtime.refresh_persisted_control()? {
                            runtime.persist()?;
                        }
                        if let Err(error) = session.send_heartbeat(&runtime) {
                            eprintln!("worker heartbeat failed: {error}");
                            runtime.mark_disconnected();
                            runtime.persist()?;
                            break;
                        }
                        runtime.mark_heartbeat_sent(now);
                        runtime.persist()?;
                    }
                }
                Err(error) => {
                    eprintln!("worker registration failed: {error}");
                    runtime.mark_disconnected();
                    runtime.persist()?;
                }
            },
            Err(error) => {
                eprintln!("worker connection failed: {error}");
                runtime.mark_disconnected();
                runtime.persist()?;
            }
        }

        if once {
            return Err("worker service could not establish its one-shot session".into());
        }
        if interruptible_sleep(runtime.reconnect_delay(), stop_requested.as_ref()) {
            runtime.request_drain();
            runtime.persist()?;
            return Ok(());
        }
    }
}

fn service_stop_requested(flag: Option<&std::sync::Arc<std::sync::atomic::AtomicBool>>) -> bool {
    flag.map(|value| value.load(std::sync::atomic::Ordering::SeqCst))
        .unwrap_or(false)
}

fn interruptible_sleep(
    duration: std::time::Duration,
    stop_requested: Option<&std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> bool {
    let deadline = std::time::Instant::now() + duration;
    while std::time::Instant::now() < deadline {
        if service_stop_requested(stop_requested) {
            return true;
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        std::thread::sleep(remaining.min(std::time::Duration::from_millis(250)));
    }
    service_stop_requested(stop_requested)
}

#[cfg(windows)]
fn worker_service_windows(_args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    windows_service::service_dispatcher::start(
        df_test_worker_service::WINDOWS_SERVICE_NAME,
        ffi_worker_service_main,
    )?;
    Ok(())
}

#[cfg(not(windows))]
fn worker_service_windows(_args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    Err("worker-service-windows is only available on Windows".into())
}

#[cfg(windows)]
fn windows_worker_service_main(_arguments: Vec<std::ffi::OsString>) {
    if let Err(error) = run_windows_worker_service() {
        eprintln!("DragonForge Windows worker service failed: {error}");
    }
}

#[cfg(windows)]
fn run_windows_worker_service() -> Result<(), Box<dyn std::error::Error>> {
    use windows_service::{
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult},
    };

    let stop_requested = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let handler_stop = stop_requested.clone();
    let event_handler = move |event| -> ServiceControlHandlerResult {
        match event {
            ServiceControl::Stop => {
                handler_stop.store(true, std::sync::atomic::Ordering::SeqCst);
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register(
        df_test_worker_service::WINDOWS_SERVICE_NAME,
        event_handler,
    )?;
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: std::time::Duration::default(),
        process_id: None,
    })?;

    let args: Vec<String> = std::env::args().skip(2).collect();
    let result = worker_service_run_loop(&args, Some(stop_requested));

    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(if result.is_ok() { 0 } else { 1 }),
        checkpoint: 0,
        wait_hint: std::time::Duration::default(),
        process_id: None,
    })?;

    result
}

fn worker_service_drain(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = value_after(args, "--config").ok_or("missing --config <worker.json>")?;
    let config = load_worker_config(config_path)?;
    let mut runtime = WorkerServiceRuntime::recover(config)?;
    runtime.request_drain();
    runtime.persist()?;
    println!("worker_id={}", runtime.snapshot().worker_id);
    println!("active_jobs={}", runtime.snapshot().active_jobs);
    println!("state=draining");
    println!("status=worker_service_drain_requested");
    Ok(())
}

fn worker_service_resume(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = value_after(args, "--config").ok_or("missing --config <worker.json>")?;
    let config = load_worker_config(config_path)?;
    let mut runtime = WorkerServiceRuntime::recover(config)?;
    runtime.cancel_drain();
    runtime.persist()?;
    println!("worker_id={}", runtime.snapshot().worker_id);
    println!("state=connecting");
    println!("status=worker_service_resume_requested");
    Ok(())
}

fn worker_service_specs(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let executable =
        value_after(args, "--executable").ok_or("missing --executable <absolute-path>")?;
    let config_path = value_after(args, "--config").ok_or("missing --config <worker.json>")?;
    let windows = WindowsServiceSpec::new(PathBuf::from(&executable), PathBuf::from(&config_path))?;
    let systemd = SystemdServiceSpec::new(PathBuf::from(&executable), PathBuf::from(&config_path))?;
    println!(
        "windows_service={}",
        serde_json::to_string_pretty(&windows)?
    );
    println!("systemd_unit_name={}", systemd.unit_name);
    println!("systemd_unit_begin");
    print!("{}", systemd.render_unit());
    println!("systemd_unit_end");
    println!("status=worker_service_specs_ready");
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
    if std::env::consts::OS == "linux" {
        let inventory = ArmInspector::host().inventory()?;
        features.extend(arm_node_features(&inventory));
    }

    let mut labels: BTreeSet<String> = ["phase8-probe".to_string()].into_iter().collect();
    if features.contains(&NodeFeature::ArmWorker) {
        labels.insert("arm".into());
    }
    if features.contains(&NodeFeature::RaspberryPi) {
        labels.insert("raspberry-pi".into());
    }

    let registration = NodeRegistration {
        protocol_version: PROTOCOL_VERSION,
        profile: NodeProfile {
            node_id,
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            labels,
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

fn mcp_doctor(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let config = mcp_config(args)?;
    let allowlist_count = config.allowed_repository_prefixes.len();
    let gateway = McpGateway::new(config)?;

    println!("DragonForge Test Lab MCP doctor");
    println!("bind={}", gateway.bind_address());
    println!("transport=loopback_http");
    println!("authentication=bearer_token_from_environment");
    println!("token_retained_in_plaintext=false");
    println!("allowed_repository_prefixes={allowlist_count}");
    println!("mcp_modern=2026-07-28");
    println!("mcp_legacy=2025-11-25");
    println!("status=mcp_gateway_ready");
    Ok(())
}

fn mcp_serve(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let config = mcp_config(args)?;
    let gateway = McpGateway::new(config)?;
    println!("DragonForge Test Lab MCP gateway");
    println!("bind={}", gateway.bind_address());
    println!("endpoint=http://{}/mcp", gateway.bind_address());
    println!("health=http://{}/health", gateway.bind_address());
    println!("status=mcp_gateway_listening");
    gateway.serve()?;
    Ok(())
}

fn mcp_fixture(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let config = mcp_config(args)?;
    let gateway = McpGateway::new(config)?;

    let unauthorized = gateway.handle_http_request(df_test_mcp::HttpRequest {
        method: "POST".into(),
        path: "/mcp".into(),
        headers: std::collections::HashMap::new(),
        body: serde_json::to_vec(&serde_json::json!({
            "jsonrpc":"2.0",
            "id":1,
            "method":"tools/list",
            "params":{}
        }))?,
    });
    if unauthorized.status != 401 {
        return Err("MCP fixture expected unauthenticated request to return 401".into());
    }

    let token = std::env::var("DRAGONFORGE_MCP_TOKEN")?;
    let mut headers = std::collections::HashMap::new();
    headers.insert("authorization".into(), format!("Bearer {token}"));
    headers.insert("mcp-protocol-version".into(), "2026-07-28".into());
    headers.insert("mcp-method".into(), "server/discover".into());

    let discovery = gateway.handle_http_request(df_test_mcp::HttpRequest {
        method: "POST".into(),
        path: "/mcp".into(),
        headers,
        body: serde_json::to_vec(&serde_json::json!({
            "jsonrpc":"2.0",
            "id":2,
            "method":"server/discover",
            "params":{
                "_meta":{
                    "io.modelcontextprotocol/protocolVersion":"2026-07-28",
                    "io.modelcontextprotocol/clientInfo":{
                        "name":"dragonforge-phase9-fixture",
                        "version":"1.0"
                    },
                    "io.modelcontextprotocol/clientCapabilities":{}
                }
            }
        }))?,
    });

    if discovery.status != 200 {
        return Err("MCP modern discovery fixture failed".into());
    }
    let body = discovery.body.ok_or("MCP discovery returned no body")?;
    if body["result"]["supportedVersions"]
        .as_array()
        .map(|versions| versions.iter().any(|version| version == "2026-07-28"))
        != Some(true)
    {
        return Err("MCP discovery did not advertise 2026-07-28".into());
    }

    println!("unauthorized_request=blocked");
    println!("modern_discovery=passed");
    println!("tool_surface=typed_only");
    println!("status=mcp_fixture_passed");
    Ok(())
}

fn mcp_config(args: &[String]) -> Result<McpGatewayConfig, Box<dyn std::error::Error>> {
    let bind: std::net::SocketAddr = value_after(args, "--bind")
        .unwrap_or_else(|| "127.0.0.1:45890".into())
        .parse()?;

    let bearer_token = std::env::var("DRAGONFORGE_MCP_TOKEN")
        .map_err(|_| "DRAGONFORGE_MCP_TOKEN must be set and at least 32 characters")?;
    if bearer_token.len() < 32 {
        return Err("DRAGONFORGE_MCP_TOKEN must be at least 32 characters".into());
    }

    let allowlist = std::env::var("DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES")
        .map_err(|_| "DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES must be set")?;
    let allowed_repository_prefixes: Vec<String> = allowlist
        .split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect();
    if allowed_repository_prefixes.is_empty() {
        return Err(
            "DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES must contain at least one HTTPS prefix"
                .into(),
        );
    }

    Ok(McpGatewayConfig {
        bind,
        bearer_token,
        allowed_repository_prefixes,
        lab_root: lab_root(args),
        sandbox_mode: sandbox_mode(args)?,
        expected_worker_user: value_after(args, "--worker-user"),
    })
}

fn intelligence_doctor() -> Result<(), Box<dyn std::error::Error>> {
    println!("DragonForge Test Lab intelligence doctor");
    println!("mode=deterministic_explainable");
    println!("change_aware_selection=true");
    println!("historical_regression_targeting=true");
    println!("failure_clustering=sha256_normalized");
    println!("resource_aware_scheduling=true");
    println!(
        "max_changed_files={}",
        df_test_intelligence::MAX_CHANGED_FILES
    );
    println!(
        "max_history_records={}",
        df_test_intelligence::MAX_HISTORY_RECORDS
    );
    println!("max_workers={}", df_test_intelligence::MAX_WORKERS);
    println!("status=test_intelligence_ready");
    Ok(())
}

fn intelligence_analyze(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let input_path = value_after(args, "--input").ok_or("missing --input <intelligence.json>")?;
    let input: IntelligenceInput = serde_json::from_slice(&std::fs::read(input_path)?)?;
    let report = analyze(&input)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    println!("status=test_intelligence_analysis_complete");
    Ok(())
}

fn intelligence_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let input = IntelligenceInput {
        changes: df_test_intelligence::ChangeSet {
            files: vec![
                "crates/df-test-mcp/src/lib.rs".into(),
                "crates/df-test-controller/src/lib.rs".into(),
            ],
        },
        history: vec![df_test_intelligence::HistoricalFailure {
            id: "00000000-0000-4000-8000-000000000010".parse()?,
            profile: TestProfile::McpGateway,
            step: "mcp integration".into(),
            message: "gateway fixture failed on port 55648".into(),
            changed_files: vec!["crates/df-test-mcp/src/lib.rs".into()],
            unix_time_secs: 1_000,
        }],
        workers: vec![
            WorkerCapacity {
                worker_id: "fixture-fast".into(),
                supported_profiles: [
                    TestProfile::RustFast,
                    TestProfile::RustStandard,
                    TestProfile::McpGateway,
                ]
                .into_iter()
                .collect(),
                total_memory_mib: 8192,
                free_memory_mib: 6144,
                max_parallel_jobs: 2,
                active_jobs: 0,
                load_percent: 10,
            },
            WorkerCapacity {
                worker_id: "fixture-deep".into(),
                supported_profiles: [TestProfile::RustDeep, TestProfile::FullRegression]
                    .into_iter()
                    .collect(),
                total_memory_mib: 16384,
                free_memory_mib: 12288,
                max_parallel_jobs: 2,
                active_jobs: 0,
                load_percent: 20,
            },
        ],
    };

    let report = analyze(&input)?;
    let mcp_selected = report
        .recommendations
        .iter()
        .any(|item| item.profile == TestProfile::McpGateway && item.score >= 70);
    let history_clustered = report
        .failure_clusters
        .iter()
        .any(|cluster| cluster.occurrences == 1);
    let mcp_scheduled = report
        .schedule
        .iter()
        .any(|item| item.profile == TestProfile::McpGateway && item.worker_id == "fixture-fast");

    println!("{}", serde_json::to_string_pretty(&report)?);
    if !mcp_selected || !history_clustered || !mcp_scheduled {
        return Err("Phase 10 intelligence fixture failed".into());
    }
    println!("status=test_intelligence_fixture_passed");
    Ok(())
}

fn intelligence_integration_doctor() -> Result<(), Box<dyn std::error::Error>> {
    let controller = DurableController::open_in_memory()?;
    if controller.schema_version()? < 5 {
        return Err("Phase 17 controller schema is unavailable".into());
    }
    if IntelligenceMode::parse("advisory")? != IntelligenceMode::Advisory
        || IntelligenceMode::parse("automatic")? != IntelligenceMode::Automatic
    {
        return Err("Phase 17 intelligence modes are invalid".into());
    }

    println!("DragonForge Test Lab intelligence integration doctor");
    println!("controller_schema={}", controller.schema_version()?);
    println!("git_change_source=github_compare");
    println!("historical_failures=durable_test_failure_context");
    println!("worker_capacity=durable_online_workers_and_live_slots");
    println!("advisory_mode=true");
    println!("automatic_mode=typed_root_plan_steps_only");
    println!("decision_audit=hash_chained");
    println!("default_automatic_score={DEFAULT_MIN_AUTOMATIC_SCORE}");
    println!("status=intelligence_integration_ready");
    Ok(())
}

fn intelligence_integrate(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let repository_url = value_after(args, "--repo").ok_or("missing --repo <github-https-url>")?;
    let base_revision = value_after(args, "--base").ok_or("missing --base <revision>")?;
    let head_revision = value_after(args, "--head").ok_or("missing --head <revision>")?;
    let plan_name = value_after(args, "--plan").ok_or("missing --plan <stored-plan-name>")?;
    let mode =
        IntelligenceMode::parse(&value_after(args, "--mode").unwrap_or_else(|| "advisory".into()))?;
    let min_automatic_score = value_after(args, "--min-score")
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(DEFAULT_MIN_AUTOMATIC_SCORE);
    let state_db = value_after(args, "--state-db")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".dragonforge-test-lab").join("controller.sqlite3"));
    if let Some(parent) = state_db.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let mut controller = DurableController::open(&state_db)?;
    let github = GhGitHubClient::default();
    github.doctor()?;
    let decision = integrate(
        &mut controller,
        &github,
        &IntegrationRequest {
            repository_url,
            base_revision,
            head_revision,
            plan_name,
            mode,
            min_automatic_score,
            now_secs,
        },
    )?;
    println!("{}", serde_json::to_string_pretty(&decision)?);
    println!("status=intelligence_integration_complete");
    Ok(())
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
    execute_typed_job(
        job,
        lab_root,
        retain_workspace,
        sandbox_mode,
        expected_worker_user,
    )
}

fn execute_typed_job(
    job: JobRequest,
    lab_root: PathBuf,
    retain_workspace: bool,
    sandbox_mode: SandboxMode,
    expected_worker_user: Option<String>,
) -> Result<ExecutionReport, Box<dyn std::error::Error>> {
    if !job.repository.url.starts_with("https://") {
        return Err("job repository must be an HTTPS URL".into());
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

    let policy = ExecutionPolicy::new(vec![job.repository.url.clone()], capabilities.clone());
    let registration = WorkerRegistration {
        worker_id: format!("local-{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        protocol_version: PROTOCOL_VERSION,
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        capabilities,
    };

    let agent = Agent::new(registration, policy)?;
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
    println!("  dragonforge-test-lab identity-doctor");
    println!("  dragonforge-test-lab identity-fixture");
    println!("  dragonforge-test-lab worker-service-doctor");
    println!("  dragonforge-test-lab worker-service-fixture");
    println!("  dragonforge-test-lab worker-service-run --config <worker.json> [--once] [--service-mode]");
    println!("  dragonforge-test-lab worker-service-windows --config <worker.json>");
    println!("  dragonforge-test-lab worker-service-drain --config <worker.json>");
    println!("  dragonforge-test-lab worker-service-resume --config <worker.json>");
    println!("  dragonforge-test-lab worker-service-specs --executable <absolute-path> --config <worker.json>");
    println!("  dragonforge-test-lab observability-doctor");
    println!("  dragonforge-test-lab observability-fixture");
    println!("  dragonforge-test-lab observability-summary [--state-db <path>]");
    println!("  dragonforge-test-lab lifecycle-doctor");
    println!("  dragonforge-test-lab lifecycle-fixture");
    println!("  dragonforge-test-lab lifecycle-status --job-id <uuid> [--state-db <path>]");
    println!("  dragonforge-test-lab lifecycle-reschedule --job-id <uuid> [--state-db <path>]");
    println!("  dragonforge-test-lab plan-doctor");
    println!("  dragonforge-test-lab plan-fixture");
    println!("  dragonforge-test-lab plan-validate --plan <plan.json>");
    println!("  dragonforge-test-lab plan-compile --plan <plan.json> --step <id>");
    println!("  dragonforge-test-lab plan-store --plan <plan.json> [--state-db <path>]");
    println!("  dragonforge-test-lab plan-list [--state-db <path>]");
    println!("  dragonforge-test-lab controller-state-doctor [--state-db <path>]");
    println!("  dragonforge-test-lab controller-state-fixture");
    println!("  dragonforge-test-lab dashboard-doctor [--bind 127.0.0.1:8788] [--state-db <path>]");
    println!("  dragonforge-test-lab dashboard-fixture");
    println!("  dragonforge-test-lab dashboard-serve [--bind 127.0.0.1:8788] [--state-db <path>]");
    println!("  dragonforge-test-lab linux-doctor");
    println!("  dragonforge-test-lab linux-fixture");
    println!("  dragonforge-test-lab arm-doctor");
    println!("  dragonforge-test-lab arm-fixture");
    println!("  dragonforge-test-lab arm-probe --probe board-model|cpu-temperature|gpio-controllers|i2c-buses|spi-devices|serial-devices");
    println!("  dragonforge-test-lab install-doctor");
    println!("  dragonforge-test-lab install-fixture");
    println!("  dragonforge-test-lab install-layout");
    println!("  dragonforge-test-lab release-verify --manifest <release-manifest.json> --package-root <directory>");
    println!("  dragonforge-test-lab upgrade-plan --state <install-state.json> --manifest <release-manifest.json>");
    println!("  dragonforge-test-lab release-doctor");
    println!("  dragonforge-test-lab release-fixture");
    println!("  dragonforge-test-lab release-tag --channel dev|beta|stable --version <version>");
    println!("  dragonforge-test-lab release-bundle-verify --manifest <release-bundle.json> --root <release-directory>");
    println!("  dragonforge-test-lab security-doctor");
    println!("  dragonforge-test-lab security-fixture");
    println!("  dragonforge-test-lab dogfood-doctor");
    println!("  dragonforge-test-lab dogfood-fixture");
    println!("  dragonforge-test-lab dogfood-profile-validate --profile <profile.json>");
    println!("  dragonforge-test-lab dogfood-profile-compile --profile <profile.json> --sha <commit> [--depth 0|1]");
    println!("  dragonforge-test-lab dogfood-campaign-validate --campaign <campaign.json>");
    println!("  dragonforge-test-lab dogfood-campaign-run --campaign <campaign.json> [--lab-root <path>] [--retain-workspace] [--sandbox native|docker|podman] [--worker-user <name>]");
    println!("  dragonforge-test-lab dogfood-run --profile <profile.json> [--revision <ref>] [--depth 0|1] [--lab-root <path>] [--retain-workspace] [--sandbox native|docker|podman] [--worker-user <name>]");
    println!(
        "  dragonforge-test-lab security-review [--root <repository-root>] [--output <report.json>]"
    );
    println!("  dragonforge-test-lab mcp-doctor [--bind 127.0.0.1:45890] [--lab-root <path>] [--sandbox native|docker|podman] [--worker-user <name>]");
    println!("  dragonforge-test-lab mcp-serve [--bind 127.0.0.1:45890] [--lab-root <path>] [--sandbox native|docker|podman] [--worker-user <name>]");
    println!("  dragonforge-test-lab mcp-fixture [--bind 127.0.0.1:45890]");
    println!("  dragonforge-test-lab intelligence-doctor");
    println!("  dragonforge-test-lab intelligence-analyze --input <intelligence.json>");
    println!("  dragonforge-test-lab intelligence-fixture");
    println!("  dragonforge-test-lab intelligence-integration-doctor");
    println!(
        "  dragonforge-test-lab intelligence-integrate --repo <github-https-url> --base <revision> --head <revision> --plan <stored-plan-name> [--mode advisory|automatic] [--min-score 60] [--state-db <path>]"
    );
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
