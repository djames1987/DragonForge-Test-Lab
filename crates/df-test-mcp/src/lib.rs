use df_test_agent::Agent;
use df_test_executor::{
    CancellationToken, ExecutionReport, ExecutorConfig, LocalExecutor, StepStatus,
};
use df_test_policy::ExecutionPolicy;
use df_test_protocol::{
    Capability, JobRequest, JobStatus, RepositorySpec, ResourceLimits, TestAction,
    WorkerRegistration, PROTOCOL_VERSION,
};
use df_test_sandbox::{verify_container_image, SandboxMode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashMap},
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use uuid::Uuid;

pub const MCP_MODERN_VERSION: &str = "2026-07-28";
pub const MCP_LEGACY_VERSION: &str = "2025-11-25";
pub const MAX_HTTP_HEADER_BYTES: usize = 16 * 1024;
pub const MAX_HTTP_BODY_BYTES: usize = 256 * 1024;
pub const MAX_GATEWAY_JOBS: usize = 1024;
pub const MAX_ACTIVE_GATEWAY_JOBS: usize = 4;
pub const MAX_ARTIFACTS_PER_JOB: usize = 256;

#[derive(Debug, Clone)]
pub struct McpGatewayConfig {
    pub bind: SocketAddr,
    pub bearer_token: String,
    pub allowed_repository_prefixes: Vec<String>,
    pub lab_root: PathBuf,
    pub sandbox_mode: SandboxMode,
    pub expected_worker_user: Option<String>,
}

impl McpGatewayConfig {
    pub fn validate(&self) -> Result<(), McpGatewayError> {
        if !self.bind.ip().is_loopback() || self.bind.port() == 0 {
            return Err(McpGatewayError::NonLoopbackBind);
        }
        if !(32..=4096).contains(&self.bearer_token.len()) {
            return Err(McpGatewayError::InvalidBearerToken);
        }
        if self.allowed_repository_prefixes.is_empty()
            || self.allowed_repository_prefixes.len() > 128
        {
            return Err(McpGatewayError::InvalidRepositoryAllowlist);
        }
        for prefix in &self.allowed_repository_prefixes {
            if !prefix.starts_with("https://")
                || prefix.len() > 512
                || prefix.bytes().any(|byte| byte.is_ascii_control())
            {
                return Err(McpGatewayError::InvalidRepositoryAllowlist);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayJobProfile {
    RustStandard,
    RustTest,
}

impl GatewayJobProfile {
    fn actions(self) -> Vec<TestAction> {
        match self {
            Self::RustStandard => vec![
                TestAction::Checkout,
                TestAction::CargoFmtCheck,
                TestAction::CargoClippy {
                    deny_warnings: true,
                },
                TestAction::CargoTest { all_features: true },
            ],
            Self::RustTest => vec![
                TestAction::Checkout,
                TestAction::CargoTest { all_features: true },
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayJobSubmission {
    pub repository: String,
    pub revision: String,
    pub profile: GatewayJobProfile,
}

impl GatewayJobSubmission {
    pub fn validate(&self, prefixes: &[String]) -> Result<(), McpGatewayError> {
        if !self.repository.starts_with("https://")
            || self.repository.len() > 1024
            || self.repository.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(McpGatewayError::InvalidRepository);
        }
        if !prefixes
            .iter()
            .any(|prefix| repository_allowed(&self.repository, prefix))
        {
            return Err(McpGatewayError::RepositoryNotAllowed);
        }
        validate_revision(&self.revision)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayJobState {
    Queued,
    Running,
    Passed,
    Failed,
    Cancelled,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayJobRecord {
    pub job_id: Uuid,
    pub repository: String,
    pub revision: String,
    pub profile: GatewayJobProfile,
    pub state: GatewayJobState,
    pub submitted_at_secs: u64,
    pub started_at_secs: Option<u64>,
    pub completed_at_secs: Option<u64>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayStepSummary {
    pub name: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u128,
    pub output_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayArtifactMetadata {
    pub name: String,
    pub relative_path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayJobResult {
    pub job_id: Uuid,
    pub status: JobStatus,
    pub summary: String,
    pub sandbox_mode: String,
    pub steps: Vec<GatewayStepSummary>,
    pub artifacts: Vec<GatewayArtifactMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayNodeInfo {
    pub node_id: String,
    pub os: String,
    pub arch: String,
    pub protocol_version: u16,
    pub status: String,
    pub capabilities: BTreeSet<Capability>,
}

#[derive(Debug, Default)]
struct GatewayState {
    jobs: HashMap<Uuid, GatewayJobRecord>,
    results: HashMap<Uuid, GatewayJobResult>,
}

#[derive(Debug)]
struct GatewayInner {
    config: McpGatewayConfig,
    token_digest: [u8; 32],
    state: Mutex<GatewayState>,
}

#[derive(Clone, Debug)]
pub struct McpGateway {
    inner: Arc<GatewayInner>,
}

impl McpGateway {
    pub fn new(config: McpGatewayConfig) -> Result<Self, McpGatewayError> {
        config.validate()?;
        let token_digest: [u8; 32] = Sha256::digest(config.bearer_token.as_bytes()).into();
        let mut stored_config = config;
        stored_config.bearer_token.clear();
        Ok(Self {
            inner: Arc::new(GatewayInner {
                config: stored_config,
                token_digest,
                state: Mutex::new(GatewayState::default()),
            }),
        })
    }

    pub fn bind_address(&self) -> SocketAddr {
        self.inner.config.bind
    }

    pub fn serve(&self) -> Result<(), McpGatewayError> {
        self.serve_requests(None)
    }

    pub fn serve_requests(&self, max_requests: Option<usize>) -> Result<(), McpGatewayError> {
        let listener = TcpListener::bind(self.inner.config.bind)?;
        let mut handled = 0usize;
        for incoming in listener.incoming() {
            let mut stream = incoming?;
            stream.set_read_timeout(Some(Duration::from_secs(15)))?;
            stream.set_write_timeout(Some(Duration::from_secs(15)))?;
            if let Err(error) = self.handle_stream(&mut stream) {
                let _ = write_http_json(
                    &mut stream,
                    500,
                    &json!({"error":"internal gateway error","detail":error.to_string()}),
                    &[],
                );
            }
            handled += 1;
            if max_requests.is_some_and(|limit| handled >= limit) {
                break;
            }
        }
        Ok(())
    }

    pub fn handle_stream(&self, stream: &mut TcpStream) -> Result<(), McpGatewayError> {
        let request = read_http_request(stream)?;
        let response = self.handle_http_request(request);
        match response.body {
            Some(body) => write_http_json(stream, response.status, &body, &response.headers)?,
            None => write_http_empty(stream, response.status, &response.headers)?,
        }
        Ok(())
    }

    pub fn handle_http_request(&self, request: HttpRequest) -> HttpResponse {
        if request.method == "GET" && request.path == "/health" {
            return HttpResponse::json(
                200,
                json!({
                    "ok": true,
                    "service": "dragonforge-test-lab-mcp",
                    "version": env!("CARGO_PKG_VERSION")
                }),
            );
        }

        if request.path != "/mcp" {
            return HttpResponse::json(404, json!({"error":"not found"}));
        }
        if request.method != "POST" {
            return HttpResponse::json(405, json!({"error":"method not allowed"}));
        }

        if !self.authorized(request.headers.get("authorization").map(String::as_str)) {
            return HttpResponse {
                status: 401,
                headers: vec![(
                    "WWW-Authenticate".into(),
                    "Bearer realm=\"DragonForge Test Lab MCP\"".into(),
                )],
                body: Some(json!({"error":"unauthorized"})),
            };
        }

        let rpc: RpcRequest = match serde_json::from_slice(&request.body) {
            Ok(value) => value,
            Err(_) => {
                return HttpResponse::json(400, rpc_error(Value::Null, -32700, "parse error", None))
            }
        };

        if let Err(error) = validate_rpc_request(&rpc) {
            return HttpResponse::json(
                400,
                rpc_error(
                    rpc.id.clone().unwrap_or(Value::Null),
                    -32600,
                    &error.to_string(),
                    None,
                ),
            );
        }

        if let Err(error) = validate_modern_headers(&request.headers, &rpc) {
            return HttpResponse::json(
                400,
                rpc_error(
                    rpc.id.clone().unwrap_or(Value::Null),
                    -32020,
                    &error.to_string(),
                    None,
                ),
            );
        }

        if rpc.id.is_none() {
            return HttpResponse {
                status: 202,
                headers: Vec::new(),
                body: None,
            };
        }

        let id = rpc.id.clone().unwrap_or(Value::Null);
        let modern = request
            .headers
            .get("mcp-protocol-version")
            .is_some_and(|value| value == MCP_MODERN_VERSION)
            || modern_meta_version(&rpc).is_some_and(|value| value == MCP_MODERN_VERSION);

        let result = match rpc.method.as_str() {
            "server/discover" => Ok(self.discover_result()),
            "initialize" => self.initialize_result(&rpc),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools": tool_definitions()})),
            "tools/call" => self.call_tool(&rpc),
            _ => Err(RpcFailure::new(-32601, "method not found")),
        };

        match result {
            Ok(mut value) => {
                if modern {
                    stamp_modern_server_info(&mut value);
                }
                HttpResponse::json(200, rpc_success(id, value))
            }
            Err(error) => {
                HttpResponse::json(200, rpc_error(id, error.code, &error.message, error.data))
            }
        }
    }

    fn authorized(&self, auth_header: Option<&str>) -> bool {
        let Some(header) = auth_header else {
            return false;
        };
        let Some(token) = header.strip_prefix("Bearer ") else {
            return false;
        };
        let candidate: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        constant_time_equal(&candidate, &self.inner.token_digest)
    }

    fn discover_result(&self) -> Value {
        json!({
            "supportedVersions": [MCP_MODERN_VERSION, MCP_LEGACY_VERSION],
            "capabilities": {"tools": {}},
            "instructions": "DragonForge Test Lab exposes authenticated typed test-lab operations only."
        })
    }

    fn initialize_result(&self, rpc: &RpcRequest) -> Result<Value, RpcFailure> {
        let requested = rpc
            .params
            .as_ref()
            .and_then(|params| params.get("protocolVersion"))
            .and_then(Value::as_str)
            .unwrap_or(MCP_LEGACY_VERSION);

        if requested != MCP_LEGACY_VERSION {
            return Err(RpcFailure::new(
                -32602,
                "initialize supports the 2025-11-25 legacy MCP revision; use server/discover for 2026-07-28",
            ));
        }

        Ok(json!({
            "protocolVersion": MCP_LEGACY_VERSION,
            "capabilities": {"tools": {}},
            "serverInfo": {
                "name": "dragonforge-test-lab",
                "version": env!("CARGO_PKG_VERSION")
            },
            "instructions": "Use named DragonForge test profiles only. Arbitrary commands are not supported."
        }))
    }

    fn call_tool(&self, rpc: &RpcRequest) -> Result<Value, RpcFailure> {
        let params = rpc
            .params
            .as_ref()
            .ok_or_else(|| RpcFailure::new(-32602, "missing tool call params"))?;
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcFailure::new(-32602, "missing tool name"))?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let structured = match name {
            "dragonforge_lab_status" => self.tool_lab_status(),
            "dragonforge_nodes_list" => self.tool_nodes_list(),
            "dragonforge_job_submit" => self.tool_job_submit(arguments),
            "dragonforge_job_status" => self.tool_job_status(arguments),
            "dragonforge_job_result" => self.tool_job_result(arguments),
            "dragonforge_artifact_list" => self.tool_artifact_list(arguments),
            _ => Err(McpGatewayError::UnknownTool(name.to_string())),
        };

        match structured {
            Ok(value) => Ok(tool_success(value)),
            Err(error) => Ok(tool_error(error.to_string())),
        }
    }

    fn tool_lab_status(&self) -> Result<Value, McpGatewayError> {
        let state = self.state_lock()?;
        let running = state
            .jobs
            .values()
            .filter(|job| {
                matches!(
                    job.state,
                    GatewayJobState::Queued | GatewayJobState::Running
                )
            })
            .count();
        Ok(json!({
            "service": "DragonForge Test Lab MCP Gateway",
            "version": env!("CARGO_PKG_VERSION"),
            "protocol_version": PROTOCOL_VERSION,
            "mcp_versions": [MCP_MODERN_VERSION, MCP_LEGACY_VERSION],
            "bind": self.inner.config.bind.to_string(),
            "jobs_total": state.jobs.len(),
            "jobs_active": running,
            "authentication": "bearer_sha256_compare",
            "loopback_only": true
        }))
    }

    fn tool_nodes_list(&self) -> Result<Value, McpGatewayError> {
        let node = local_node_info();
        Ok(serde_json::to_value(vec![node])?)
    }

    fn tool_job_submit(&self, arguments: Value) -> Result<Value, McpGatewayError> {
        let submission: GatewayJobSubmission = serde_json::from_value(arguments)?;
        submission.validate(&self.inner.config.allowed_repository_prefixes)?;

        let job_id = Uuid::new_v4();
        let now = unix_time_secs()?;
        let record = GatewayJobRecord {
            job_id,
            repository: submission.repository.clone(),
            revision: submission.revision.clone(),
            profile: submission.profile,
            state: GatewayJobState::Queued,
            submitted_at_secs: now,
            started_at_secs: None,
            completed_at_secs: None,
            summary: "queued".into(),
        };

        {
            let mut state = self.state_lock()?;
            if state.jobs.len() >= MAX_GATEWAY_JOBS {
                prune_completed_jobs(&mut state);
            }
            if state.jobs.len() >= MAX_GATEWAY_JOBS {
                return Err(McpGatewayError::JobCapacityExceeded);
            }
            let active_jobs = state
                .jobs
                .values()
                .filter(|job| {
                    matches!(
                        job.state,
                        GatewayJobState::Queued | GatewayJobState::Running
                    )
                })
                .count();
            if active_jobs >= MAX_ACTIVE_GATEWAY_JOBS {
                return Err(McpGatewayError::ActiveJobLimitExceeded);
            }
            state.jobs.insert(job_id, record);
        }

        let profile = submission.profile;
        let gateway = self.clone();
        let _job_thread = thread::spawn(move || {
            gateway.run_job(job_id, submission);
        });

        Ok(json!({
            "job_id": job_id,
            "state": "queued",
            "profile": profile
        }))
    }

    fn tool_job_status(&self, arguments: Value) -> Result<Value, McpGatewayError> {
        let job_id = parse_job_id(&arguments)?;
        let state = self.state_lock()?;
        let job = state
            .jobs
            .get(&job_id)
            .ok_or(McpGatewayError::UnknownJob(job_id))?;
        Ok(serde_json::to_value(job)?)
    }

    fn tool_job_result(&self, arguments: Value) -> Result<Value, McpGatewayError> {
        let job_id = parse_job_id(&arguments)?;
        let state = self.state_lock()?;
        let job = state
            .jobs
            .get(&job_id)
            .ok_or(McpGatewayError::UnknownJob(job_id))?;
        if let Some(result) = state.results.get(&job_id) {
            return Ok(serde_json::to_value(result)?);
        }
        Ok(json!({
            "job_id": job_id,
            "state": job.state,
            "ready": false,
            "summary": job.summary
        }))
    }

    fn tool_artifact_list(&self, arguments: Value) -> Result<Value, McpGatewayError> {
        let job_id = parse_job_id(&arguments)?;
        let state = self.state_lock()?;
        if !state.jobs.contains_key(&job_id) {
            return Err(McpGatewayError::UnknownJob(job_id));
        }
        let artifacts = state
            .results
            .get(&job_id)
            .map(|result| result.artifacts.clone())
            .unwrap_or_default();
        Ok(json!({
            "job_id": job_id,
            "artifacts": artifacts
        }))
    }

    fn run_job(&self, job_id: Uuid, submission: GatewayJobSubmission) {
        if let Err(error) = self.mark_job_running(job_id) {
            let _ = self.mark_job_error(job_id, error.to_string());
            return;
        }

        match execute_gateway_job(&self.inner.config, job_id, &submission) {
            Ok(result) => {
                let _ = self.finish_job(job_id, result);
            }
            Err(error) => {
                let _ = self.mark_job_error(job_id, error.to_string());
            }
        }
    }

    fn mark_job_running(&self, job_id: Uuid) -> Result<(), McpGatewayError> {
        let mut state = self.state_lock()?;
        let job = state
            .jobs
            .get_mut(&job_id)
            .ok_or(McpGatewayError::UnknownJob(job_id))?;
        job.state = GatewayJobState::Running;
        job.started_at_secs = Some(unix_time_secs()?);
        job.summary = "running".into();
        Ok(())
    }

    fn finish_job(&self, job_id: Uuid, result: GatewayJobResult) -> Result<(), McpGatewayError> {
        let mut state = self.state_lock()?;
        let job = state
            .jobs
            .get_mut(&job_id)
            .ok_or(McpGatewayError::UnknownJob(job_id))?;
        job.state = gateway_state_from_job_status(result.status);
        job.completed_at_secs = Some(unix_time_secs()?);
        job.summary = result.summary.clone();
        state.results.insert(job_id, result);
        Ok(())
    }

    fn mark_job_error(&self, job_id: Uuid, summary: String) -> Result<(), McpGatewayError> {
        let mut state = self.state_lock()?;
        if let Some(job) = state.jobs.get_mut(&job_id) {
            job.state = GatewayJobState::Error;
            job.completed_at_secs = Some(unix_time_secs()?);
            job.summary = bounded_text(&summary, 2048);
        }
        Ok(())
    }

    fn state_lock(&self) -> Result<std::sync::MutexGuard<'_, GatewayState>, McpGatewayError> {
        self.inner
            .state
            .lock()
            .map_err(|_| McpGatewayError::StatePoisoned)
    }
}

fn execute_gateway_job(
    config: &McpGatewayConfig,
    job_id: Uuid,
    submission: &GatewayJobSubmission,
) -> Result<GatewayJobResult, McpGatewayError> {
    submission.validate(&config.allowed_repository_prefixes)?;
    verify_container_image(config.sandbox_mode)?;

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

    let policy = ExecutionPolicy::new(
        config.allowed_repository_prefixes.clone(),
        capabilities.clone(),
    );
    let registration = WorkerRegistration {
        worker_id: format!(
            "mcp-local-{}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
        protocol_version: PROTOCOL_VERSION,
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        capabilities,
    };
    let agent = Agent::new(registration, policy)?;

    let job = JobRequest {
        id: job_id,
        repository: RepositorySpec {
            url: submission.repository.clone(),
            revision: submission.revision.clone(),
        },
        actions: submission.profile.actions(),
        limits: ResourceLimits::default(),
    };
    agent.validate_job(&job)?;

    let mut executor_config = ExecutorConfig::under(&config.lab_root);
    executor_config.retain_workspace = false;
    executor_config.sandbox_mode = config.sandbox_mode;
    executor_config.expected_worker_user = config.expected_worker_user.clone();

    let executor = LocalExecutor::new(executor_config);
    let cancellation = CancellationToken::new();
    let report = executor.execute(&job, &cancellation)?;
    gateway_result_from_execution(&report)
}

fn gateway_result_from_execution(
    report: &ExecutionReport,
) -> Result<GatewayJobResult, McpGatewayError> {
    let artifact_dir = PathBuf::from(&report.artifact_directory);
    let artifacts = collect_artifact_metadata(&artifact_dir)?;

    let steps = report
        .steps
        .iter()
        .map(|step| GatewayStepSummary {
            name: step.name.clone(),
            status: step_status_name(step.status).into(),
            exit_code: step.exit_code,
            duration_ms: step.duration_ms,
            output_truncated: step.output_truncated,
        })
        .collect();

    Ok(GatewayJobResult {
        job_id: report.job_id,
        status: report.status,
        summary: report.summary.clone(),
        sandbox_mode: report.sandbox_mode.clone(),
        steps,
        artifacts,
    })
}

fn collect_artifact_metadata(root: &Path) -> Result<Vec<GatewayArtifactMetadata>, McpGatewayError> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let canonical_root = fs::canonicalize(root)?;
    let mut files = Vec::new();
    collect_files_recursive(&canonical_root, &canonical_root, &mut files)?;
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    if files.len() > MAX_ARTIFACTS_PER_JOB {
        files.truncate(MAX_ARTIFACTS_PER_JOB);
    }
    Ok(files)
}

fn collect_files_recursive(
    root: &Path,
    current: &Path,
    output: &mut Vec<GatewayArtifactMetadata>,
) -> Result<(), McpGatewayError> {
    if output.len() >= MAX_ARTIFACTS_PER_JOB {
        return Ok(());
    }
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            collect_files_recursive(root, &path, output)?;
        } else if metadata.is_file() {
            let canonical = fs::canonicalize(&path)?;
            if !canonical.starts_with(root) {
                return Err(McpGatewayError::ArtifactEscapesRoot);
            }
            let relative = canonical
                .strip_prefix(root)
                .map_err(|_| McpGatewayError::ArtifactEscapesRoot)?;
            let bytes = fs::read(&canonical)?;
            output.push(GatewayArtifactMetadata {
                name: canonical
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("artifact")
                    .to_string(),
                relative_path: relative.to_string_lossy().replace('\\', "/"),
                size_bytes: metadata.len(),
                sha256: hex::encode(Sha256::digest(&bytes)),
            });
            if output.len() >= MAX_ARTIFACTS_PER_JOB {
                break;
            }
        }
    }
    Ok(())
}

fn local_node_info() -> GatewayNodeInfo {
    GatewayNodeInfo {
        node_id: format!(
            "mcp-local-{}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        protocol_version: PROTOCOL_VERSION,
        status: "online".into(),
        capabilities: [
            Capability::CheckoutRepository,
            Capability::CargoBuild,
            Capability::CargoTest,
            Capability::CargoClippy,
            Capability::CargoFmtCheck,
            Capability::ReadArtifacts,
        ]
        .into_iter()
        .collect(),
    }
}

fn gateway_state_from_job_status(status: JobStatus) -> GatewayJobState {
    match status {
        JobStatus::Passed => GatewayJobState::Passed,
        JobStatus::Cancelled => GatewayJobState::Cancelled,
        JobStatus::Failed | JobStatus::Rejected => GatewayJobState::Failed,
        JobStatus::Queued | JobStatus::Assigned | JobStatus::Running => GatewayJobState::Running,
    }
}

fn step_status_name(status: StepStatus) -> &'static str {
    match status {
        StepStatus::Passed => "passed",
        StepStatus::Failed => "failed",
        StepStatus::Cancelled => "cancelled",
        StepStatus::TimedOut => "timed_out",
    }
}

fn parse_job_id(arguments: &Value) -> Result<Uuid, McpGatewayError> {
    let value = arguments
        .get("job_id")
        .and_then(Value::as_str)
        .ok_or(McpGatewayError::MissingJobId)?;
    Uuid::parse_str(value).map_err(|_| McpGatewayError::InvalidJobId)
}

fn repository_allowed(repository: &str, prefix: &str) -> bool {
    if prefix.ends_with('/') {
        return repository.starts_with(prefix);
    }

    let repository = repository.strip_suffix(".git").unwrap_or(repository);
    let prefix = prefix.strip_suffix(".git").unwrap_or(prefix);
    repository == prefix
}

fn validate_revision(value: &str) -> Result<(), McpGatewayError> {
    if value.is_empty()
        || value.len() > 256
        || value.starts_with('-')
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err(McpGatewayError::InvalidRevision);
    }
    Ok(())
}

fn prune_completed_jobs(state: &mut GatewayState) {
    let mut completed: Vec<(Uuid, u64)> = state
        .jobs
        .values()
        .filter_map(|job| job.completed_at_secs.map(|time| (job.job_id, time)))
        .collect();
    completed.sort_by_key(|(_, time)| *time);
    let remove_count = completed.len().min(MAX_GATEWAY_JOBS / 4);
    for (job_id, _) in completed.into_iter().take(remove_count) {
        state.jobs.remove(&job_id);
        state.results.remove(&job_id);
    }
}

fn unix_time_secs() -> Result<u64, McpGatewayError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| McpGatewayError::SystemClock)?
        .as_secs())
}

fn bounded_text(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Option<Value>,
}

impl HttpResponse {
    fn json(status: u16, body: Value) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Some(body),
        }
    }
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, McpGatewayError> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Err(McpGatewayError::UnexpectedHttpEof);
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > MAX_HTTP_HEADER_BYTES + MAX_HTTP_BODY_BYTES {
            return Err(McpGatewayError::HttpRequestTooLarge);
        }
        if let Some(index) = find_subsequence(&bytes, b"\r\n\r\n") {
            if index > MAX_HTTP_HEADER_BYTES {
                return Err(McpGatewayError::HttpHeadersTooLarge);
            }
            break index + 4;
        }
    };

    let header_text =
        std::str::from_utf8(&bytes[..header_end]).map_err(|_| McpGatewayError::InvalidHttp)?;
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().ok_or(McpGatewayError::InvalidHttp)?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .ok_or(McpGatewayError::InvalidHttp)?
        .to_ascii_uppercase();
    let path = request_parts
        .next()
        .ok_or(McpGatewayError::InvalidHttp)?
        .to_string();
    let version = request_parts.next().ok_or(McpGatewayError::InvalidHttp)?;
    if !matches!(version, "HTTP/1.1" | "HTTP/1.0") {
        return Err(McpGatewayError::InvalidHttp);
    }

    let mut headers = HashMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let Some((name, value)) = line.split_once(':') else {
            return Err(McpGatewayError::InvalidHttp);
        };
        let name = name.trim().to_ascii_lowercase();
        match headers.entry(name) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(value.trim().to_string());
            }
            std::collections::hash_map::Entry::Occupied(_) => {
                return Err(McpGatewayError::DuplicateHttpHeader);
            }
        }
    }

    if headers.contains_key("transfer-encoding") {
        return Err(McpGatewayError::UnsupportedTransferEncoding);
    }

    let content_length = headers
        .get("content-length")
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| McpGatewayError::InvalidHttp)?
        .unwrap_or(0);
    if content_length > MAX_HTTP_BODY_BYTES {
        return Err(McpGatewayError::HttpBodyTooLarge);
    }

    while bytes.len().saturating_sub(header_end) < content_length {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Err(McpGatewayError::UnexpectedHttpEof);
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len().saturating_sub(header_end) > MAX_HTTP_BODY_BYTES {
            return Err(McpGatewayError::HttpBodyTooLarge);
        }
    }

    Ok(HttpRequest {
        method,
        path,
        headers,
        body: bytes[header_end..header_end + content_length].to_vec(),
    })
}

fn write_http_json(
    stream: &mut TcpStream,
    status: u16,
    body: &Value,
    extra_headers: &[(String, String)],
) -> Result<(), McpGatewayError> {
    let bytes = serde_json::to_vec(body)?;
    write_http_response(stream, status, Some(&bytes), extra_headers)
}

fn write_http_empty(
    stream: &mut TcpStream,
    status: u16,
    extra_headers: &[(String, String)],
) -> Result<(), McpGatewayError> {
    write_http_response(stream, status, None, extra_headers)
}

fn write_http_response(
    stream: &mut TcpStream,
    status: u16,
    body: Option<&[u8]>,
    extra_headers: &[(String, String)],
) -> Result<(), McpGatewayError> {
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "Response",
    };
    let body_len = body.map_or(0, |value| value.len());
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {body_len}\r\nConnection: close\r\n"
    );
    if body.is_some() {
        response.push_str("Content-Type: application/json\r\n");
    }
    for (name, value) in extra_headers {
        response.push_str(name);
        response.push_str(": ");
        response.push_str(value);
        response.push_str("\r\n");
    }
    response.push_str("\r\n");
    stream.write_all(response.as_bytes())?;
    if let Some(body) = body {
        stream.write_all(body)?;
    }
    stream.flush()?;
    Ok(())
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[derive(Debug, Clone, Deserialize)]
struct RpcRequest {
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

fn validate_rpc_request(rpc: &RpcRequest) -> Result<(), McpGatewayError> {
    if rpc.jsonrpc != "2.0" || rpc.method.is_empty() || rpc.method.len() > 128 {
        return Err(McpGatewayError::InvalidRpc);
    }
    Ok(())
}

fn modern_meta_version(rpc: &RpcRequest) -> Option<&str> {
    rpc.params
        .as_ref()?
        .get("_meta")?
        .get("io.modelcontextprotocol/protocolVersion")?
        .as_str()
}

fn validate_modern_headers(
    headers: &HashMap<String, String>,
    rpc: &RpcRequest,
) -> Result<(), McpGatewayError> {
    let modern = headers
        .get("mcp-protocol-version")
        .is_some_and(|value| value == MCP_MODERN_VERSION)
        || modern_meta_version(rpc).is_some_and(|value| value == MCP_MODERN_VERSION);
    if !modern {
        return Ok(());
    }

    let meta = rpc
        .params
        .as_ref()
        .and_then(|params| params.get("_meta"))
        .and_then(Value::as_object)
        .ok_or(McpGatewayError::McpHeaderMismatch)?;
    if meta
        .get("io.modelcontextprotocol/protocolVersion")
        .and_then(Value::as_str)
        != Some(MCP_MODERN_VERSION)
        || !meta
            .get("io.modelcontextprotocol/clientCapabilities")
            .is_some_and(Value::is_object)
    {
        return Err(McpGatewayError::McpHeaderMismatch);
    }

    if headers.get("mcp-protocol-version").map(String::as_str) != Some(MCP_MODERN_VERSION) {
        return Err(McpGatewayError::McpHeaderMismatch);
    }
    if headers.get("mcp-method").map(String::as_str) != Some(rpc.method.as_str()) {
        return Err(McpGatewayError::McpHeaderMismatch);
    }
    if rpc.method == "tools/call" {
        let name = rpc
            .params
            .as_ref()
            .and_then(|params| params.get("name"))
            .and_then(Value::as_str)
            .ok_or(McpGatewayError::McpHeaderMismatch)?;
        if headers.get("mcp-name").map(String::as_str) != Some(name) {
            return Err(McpGatewayError::McpHeaderMismatch);
        }
    }
    Ok(())
}

fn rpc_success(id: Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}

fn rpc_error(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut error = Map::new();
    error.insert("code".into(), json!(code));
    error.insert("message".into(), json!(message));
    if let Some(data) = data {
        error.insert("data".into(), data);
    }
    json!({"jsonrpc":"2.0","id":id,"error":Value::Object(error)})
}

fn stamp_modern_server_info(value: &mut Value) {
    if let Value::Object(object) = value {
        object
            .entry("resultType")
            .or_insert_with(|| Value::String("complete".into()));
        let meta = object.entry("_meta").or_insert_with(|| json!({}));
        if let Value::Object(meta_object) = meta {
            meta_object.insert(
                "io.modelcontextprotocol/serverInfo".into(),
                json!({
                    "name": "dragonforge-test-lab",
                    "version": env!("CARGO_PKG_VERSION")
                }),
            );
        }
    }
}

#[derive(Debug)]
struct RpcFailure {
    code: i64,
    message: String,
    data: Option<Value>,
}

impl RpcFailure {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}

fn tool_success(structured: Value) -> Value {
    let text = serde_json::to_string_pretty(&structured).unwrap_or_else(|_| "{}".into());
    json!({
        "content": [{"type":"text","text":text}],
        "structuredContent": structured,
        "isError": false
    })
}

fn tool_error(message: String) -> Value {
    json!({
        "content": [{"type":"text","text":message}],
        "isError": true
    })
}

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name":"dragonforge_lab_status",
            "description":"Return DragonForge Test Lab MCP gateway health and job counts.",
            "inputSchema":{"type":"object","properties":{},"additionalProperties":false}
        }),
        json!({
            "name":"dragonforge_nodes_list",
            "description":"List authorized Test Lab worker nodes visible to this gateway.",
            "inputSchema":{"type":"object","properties":{},"additionalProperties":false}
        }),
        json!({
            "name":"dragonforge_job_submit",
            "description":"Submit a typed DragonForge Rust validation profile. Arbitrary commands are not accepted.",
            "inputSchema":{
                "type":"object",
                "properties":{
                    "repository":{"type":"string","description":"Allowlisted HTTPS repository URL."},
                    "revision":{"type":"string","description":"Git revision/ref without whitespace or option prefixes."},
                    "profile":{"type":"string","enum":["rust_standard","rust_test"]}
                },
                "required":["repository","revision","profile"],
                "additionalProperties":false
            }
        }),
        json!({
            "name":"dragonforge_job_status",
            "description":"Return queued/running/completed status for a submitted Test Lab job.",
            "inputSchema":{
                "type":"object",
                "properties":{"job_id":{"type":"string","format":"uuid"}},
                "required":["job_id"],
                "additionalProperties":false
            }
        }),
        json!({
            "name":"dragonforge_job_result",
            "description":"Return a completed Test Lab result summary and step metadata without raw command access.",
            "inputSchema":{
                "type":"object",
                "properties":{"job_id":{"type":"string","format":"uuid"}},
                "required":["job_id"],
                "additionalProperties":false
            }
        }),
        json!({
            "name":"dragonforge_artifact_list",
            "description":"List SHA-256-tracked artifact metadata for a Test Lab job. File contents are not exposed.",
            "inputSchema":{
                "type":"object",
                "properties":{"job_id":{"type":"string","format":"uuid"}},
                "required":["job_id"],
                "additionalProperties":false
            }
        }),
    ]
}

fn constant_time_equal(left: &[u8; 32], right: &[u8; 32]) -> bool {
    let mut difference = 0u8;
    for (left_byte, right_byte) in left.iter().zip(right.iter()) {
        difference |= *left_byte ^ *right_byte;
    }
    difference == 0
}

#[derive(Debug, Error)]
pub enum McpGatewayError {
    #[error("MCP gateway may bind only to a loopback address with a nonzero port")]
    NonLoopbackBind,
    #[error("MCP bearer token must be between 32 and 4096 bytes")]
    InvalidBearerToken,
    #[error("MCP repository allowlist is invalid")]
    InvalidRepositoryAllowlist,
    #[error("repository URL is invalid")]
    InvalidRepository,
    #[error("repository is not allowlisted for MCP execution")]
    RepositoryNotAllowed,
    #[error("revision is invalid")]
    InvalidRevision,
    #[error("gateway job capacity exceeded")]
    JobCapacityExceeded,
    #[error("gateway concurrent job limit exceeded")]
    ActiveJobLimitExceeded,
    #[error("unknown gateway job: {0}")]
    UnknownJob(Uuid),
    #[error("missing job_id")]
    MissingJobId,
    #[error("invalid job_id")]
    InvalidJobId,
    #[error("unknown MCP tool: {0}")]
    UnknownTool(String),
    #[error("gateway state lock was poisoned")]
    StatePoisoned,
    #[error("artifact escaped configured artifact root")]
    ArtifactEscapesRoot,
    #[error("system clock is before Unix epoch")]
    SystemClock,
    #[error("HTTP request is invalid")]
    InvalidHttp,
    #[error("duplicate HTTP headers are not accepted")]
    DuplicateHttpHeader,
    #[error("HTTP transfer-encoding is not supported; use a bounded Content-Length")]
    UnsupportedTransferEncoding,
    #[error("HTTP request ended unexpectedly")]
    UnexpectedHttpEof,
    #[error("HTTP headers exceed configured limit")]
    HttpHeadersTooLarge,
    #[error("HTTP body exceeds configured limit")]
    HttpBodyTooLarge,
    #[error("HTTP request exceeds configured limit")]
    HttpRequestTooLarge,
    #[error("invalid JSON-RPC request")]
    InvalidRpc,
    #[error("MCP modern transport headers do not match the JSON-RPC request")]
    McpHeaderMismatch,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Agent(#[from] df_test_agent::AgentError),
    #[error(transparent)]
    Executor(#[from] df_test_executor::ExecutorError),
    #[error(transparent)]
    Sandbox(#[from] df_test_sandbox::SandboxError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> McpGatewayConfig {
        McpGatewayConfig {
            bind: "127.0.0.1:45890".parse().unwrap(),
            bearer_token: "0123456789abcdef0123456789abcdef".into(),
            allowed_repository_prefixes: vec!["https://github.com/djames1987/".into()],
            lab_root: PathBuf::from(".dragonforge-test-lab-test"),
            sandbox_mode: SandboxMode::Native,
            expected_worker_user: None,
        }
    }

    fn auth_headers() -> HashMap<String, String> {
        [(
            "authorization".into(),
            "Bearer 0123456789abcdef0123456789abcdef".into(),
        )]
        .into_iter()
        .collect()
    }

    #[test]
    fn gateway_does_not_retain_plaintext_bearer_token() {
        let gateway = McpGateway::new(config()).unwrap();
        assert!(gateway.inner.config.bearer_token.is_empty());
        assert_ne!(gateway.inner.token_digest, [0u8; 32]);
    }

    #[test]
    fn wrong_bearer_token_is_rejected() {
        let gateway = McpGateway::new(config()).unwrap();
        let mut headers = HashMap::new();
        headers.insert(
            "authorization".into(),
            "Bearer abcdefghijklmnopqrstuvwxyz012345".into(),
        );
        let response = gateway.handle_http_request(HttpRequest {
            method: "POST".into(),
            path: "/mcp".into(),
            headers,
            body: serde_json::to_vec(&json!({
                "jsonrpc":"2.0",
                "id":1,
                "method":"tools/list",
                "params":{}
            }))
            .unwrap(),
        });
        assert_eq!(response.status, 401);
    }

    #[test]
    fn gateway_rejects_non_loopback_bind() {
        let mut value = config();
        value.bind = "0.0.0.0:45890".parse().unwrap();
        assert!(matches!(
            McpGateway::new(value),
            Err(McpGatewayError::NonLoopbackBind)
        ));
    }

    #[test]
    fn repository_submission_is_allowlisted_and_typed() {
        let valid = GatewayJobSubmission {
            repository: "https://github.com/djames1987/project.git".into(),
            revision: "main".into(),
            profile: GatewayJobProfile::RustStandard,
        };
        assert!(valid
            .validate(&["https://github.com/djames1987/".into()])
            .is_ok());

        let invalid = GatewayJobSubmission {
            repository: "https://github.com/other/project.git".into(),
            revision: "main".into(),
            profile: GatewayJobProfile::RustTest,
        };
        assert!(matches!(
            invalid.validate(&["https://github.com/djames1987/".into()]),
            Err(McpGatewayError::RepositoryNotAllowed)
        ));

        let lookalike = GatewayJobSubmission {
            repository: "https://github.com/djames1987/project-evil.git".into(),
            revision: "main".into(),
            profile: GatewayJobProfile::RustTest,
        };
        assert!(matches!(
            lookalike.validate(&["https://github.com/djames1987/project.git".into()]),
            Err(McpGatewayError::RepositoryNotAllowed)
        ));
    }

    #[test]
    fn unauthenticated_mcp_requests_are_rejected() {
        let gateway = McpGateway::new(config()).unwrap();
        let response = gateway.handle_http_request(HttpRequest {
            method: "POST".into(),
            path: "/mcp".into(),
            headers: HashMap::new(),
            body: serde_json::to_vec(&json!({
                "jsonrpc":"2.0",
                "id":1,
                "method":"tools/list",
                "params":{}
            }))
            .unwrap(),
        });
        assert_eq!(response.status, 401);
    }

    #[test]
    fn authenticated_tools_list_exposes_only_named_tools() {
        let gateway = McpGateway::new(config()).unwrap();
        let response = gateway.handle_http_request(HttpRequest {
            method: "POST".into(),
            path: "/mcp".into(),
            headers: auth_headers(),
            body: serde_json::to_vec(&json!({
                "jsonrpc":"2.0",
                "id":1,
                "method":"tools/list",
                "params":{}
            }))
            .unwrap(),
        });
        assert_eq!(response.status, 200);
        let body = response.body.unwrap();
        let tools = body["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 6);
        assert!(tools
            .iter()
            .all(|tool| !tool["name"].as_str().unwrap().contains("shell")));
    }

    #[test]
    fn modern_requests_require_matching_transport_headers() {
        let gateway = McpGateway::new(config()).unwrap();
        let mut headers = auth_headers();
        headers.insert("mcp-protocol-version".into(), MCP_MODERN_VERSION.into());
        headers.insert("mcp-method".into(), "tools/call".into());
        headers.insert("mcp-name".into(), "wrong-name".into());
        let response = gateway.handle_http_request(HttpRequest {
            method: "POST".into(),
            path: "/mcp".into(),
            headers,
            body: serde_json::to_vec(&json!({
                "jsonrpc":"2.0",
                "id":1,
                "method":"tools/call",
                "params":{
                    "name":"dragonforge_lab_status",
                    "arguments":{},
                    "_meta":{"io.modelcontextprotocol/protocolVersion":MCP_MODERN_VERSION}
                }
            }))
            .unwrap(),
        });
        assert_eq!(response.status, 400);
        assert_eq!(response.body.unwrap()["error"]["code"], -32020);
    }

    #[test]
    fn modern_discovery_advertises_both_supported_revisions() {
        let gateway = McpGateway::new(config()).unwrap();
        let mut headers = auth_headers();
        headers.insert("mcp-protocol-version".into(), MCP_MODERN_VERSION.into());
        headers.insert("mcp-method".into(), "server/discover".into());
        let response = gateway.handle_http_request(HttpRequest {
            method: "POST".into(),
            path: "/mcp".into(),
            headers,
            body: serde_json::to_vec(&json!({
                "jsonrpc":"2.0",
                "id":1,
                "method":"server/discover",
                "params":{
                    "_meta":{
                        "io.modelcontextprotocol/protocolVersion":MCP_MODERN_VERSION,
                        "io.modelcontextprotocol/clientCapabilities":{}
                    }
                }
            }))
            .unwrap(),
        });
        assert_eq!(response.status, 200);
        let body = response.body.unwrap();
        let result = &body["result"];
        assert!(result["supportedVersions"]
            .as_array()
            .unwrap()
            .contains(&json!(MCP_MODERN_VERSION)));
    }
}
