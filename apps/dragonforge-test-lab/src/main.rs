use df_test_agent::Agent;
use df_test_controller::{DurableController, DurableJobState, SCHEMA_VERSION};
use df_test_distributed::{
    connect_registration_probe, run_distributed_fixtures, serve_registration_probe_once,
    validate_controller_addr, NodeFeature, NodeProfile, NodeRegistration,
};
use df_test_executor::{CancellationToken, ExecutionReport, ExecutorConfig, LocalExecutor};
use df_test_github::{CommitStatus, CommitStatusState, GhGitHubClient, GitHubRepository};
use df_test_gui::{GuiAutomationClient, GuiPlan};
use df_test_identity::{run_mtls_fixture, validate_private_controller_address};
use df_test_intelligence::{analyze, IntelligenceInput, TestProfile, WorkerCapacity};
use df_test_mcp::{McpGateway, McpGatewayConfig};
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
        "controller-state-doctor" => controller_state_doctor(&args[2..]),
        "controller-state-fixture" => controller_state_fixture(),
        "github-doctor" => github_doctor(),
        "identity-doctor" => identity_doctor(),
        "identity-fixture" => identity_fixture(),
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
    println!("phase=12");

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
        return Err("mTLS identity controller address policy unexpectedly allowed a public IP".into());
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
    println!("  dragonforge-test-lab identity-doctor");
    println!("  dragonforge-test-lab identity-fixture");
    println!("  dragonforge-test-lab controller-state-doctor [--state-db <path>]");
    println!("  dragonforge-test-lab controller-state-fixture");
    println!("  dragonforge-test-lab mcp-doctor [--bind 127.0.0.1:45890] [--lab-root <path>] [--sandbox native|docker|podman] [--worker-user <name>]");
    println!("  dragonforge-test-lab mcp-serve [--bind 127.0.0.1:45890] [--lab-root <path>] [--sandbox native|docker|podman] [--worker-user <name>]");
    println!("  dragonforge-test-lab mcp-fixture [--bind 127.0.0.1:45890]");
    println!("  dragonforge-test-lab intelligence-doctor");
    println!("  dragonforge-test-lab intelligence-analyze --input <intelligence.json>");
    println!("  dragonforge-test-lab intelligence-fixture");
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
