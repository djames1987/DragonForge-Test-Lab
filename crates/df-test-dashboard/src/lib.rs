use df_test_controller::{DurableController, DurableJobState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    time::Duration,
};
use thiserror::Error;

pub const DEFAULT_DASHBOARD_BIND: &str = "127.0.0.1:8788";
pub const MAX_HTTP_HEADER_BYTES: usize = 16 * 1024;
pub const MAX_HTTP_BODY_BYTES: usize = 64 * 1024;
pub const MAX_DASHBOARD_ROWS: usize = 100;

#[derive(Debug, Clone)]
pub struct DashboardConfig {
    pub bind: SocketAddr,
    pub bearer_token: String,
    pub state_db: PathBuf,
}

impl DashboardConfig {
    pub fn validate(&self) -> Result<(), DashboardError> {
        if !self.bind.ip().is_loopback() || self.bind.port() == 0 {
            return Err(DashboardError::NonLoopbackBind);
        }
        if !(32..=4096).contains(&self.bearer_token.len()) {
            return Err(DashboardError::InvalidBearerToken);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Dashboard {
    config: DashboardConfig,
    token_digest: [u8; 32],
}

impl Dashboard {
    pub fn new(config: DashboardConfig) -> Result<Self, DashboardError> {
        config.validate()?;
        let token_digest: [u8; 32] = Sha256::digest(config.bearer_token.as_bytes()).into();
        let mut stored = config;
        stored.bearer_token.clear();
        Ok(Self {
            config: stored,
            token_digest,
        })
    }

    pub fn bind_address(&self) -> SocketAddr {
        self.config.bind
    }

    pub fn serve(&self) -> Result<(), DashboardError> {
        let listener = TcpListener::bind(self.config.bind)?;
        for incoming in listener.incoming() {
            let mut stream = incoming?;
            stream.set_read_timeout(Some(Duration::from_secs(15)))?;
            stream.set_write_timeout(Some(Duration::from_secs(15)))?;
            let request = read_http_request(&mut stream)?;
            let response = self.handle_request(request);
            write_http_response(&mut stream, response)?;
        }
        Ok(())
    }

    pub fn handle_request(&self, request: HttpRequest) -> HttpResponse {
        if request.method != "GET" {
            return HttpResponse::json(405, json!({"error":"method not allowed"}));
        }

        match request.path.as_str() {
            "/" => HttpResponse::html(200, DASHBOARD_HTML),
            "/dashboard.js" => HttpResponse::javascript(200, DASHBOARD_JS),
            "/dashboard.css" => HttpResponse::css(200, DASHBOARD_CSS),
            "/health" => HttpResponse::json(
                200,
                json!({
                    "ok": true,
                    "service": "dragonforge-test-lab-dashboard",
                    "version": env!("CARGO_PKG_VERSION"),
                    "loopback_only": true
                }),
            ),
            path if path.starts_with("/api/") => {
                if !matches!(
                    path,
                    "/api/overview"
                        | "/api/jobs"
                        | "/api/workers"
                        | "/api/plans"
                        | "/api/artifacts"
                        | "/api/intelligence"
                        | "/api/audit"
                        | "/api/settings"
                ) {
                    return HttpResponse::json(404, json!({"error":"not found"}));
                }
                if !self.authorized(request.headers.get("authorization").map(String::as_str)) {
                    return HttpResponse::unauthorized();
                }
                match self.api_response(path) {
                    Ok(value) => HttpResponse::json(200, value),
                    Err(error) => HttpResponse::json(
                        500,
                        json!({"error":"dashboard query failed","detail":error.to_string()}),
                    ),
                }
            }
            _ => HttpResponse::json(404, json!({"error":"not found"})),
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
        constant_time_equal(&candidate, &self.token_digest)
    }

    fn api_response(&self, path: &str) -> Result<Value, DashboardError> {
        let controller = DurableController::open(&self.config.state_db)?;
        match path {
            "/api/overview" => {
                let jobs = controller.recent_jobs(MAX_DASHBOARD_ROWS)?;
                let workers = controller.list_workers()?;
                let plans = controller.list_test_plans()?;
                let artifacts = controller.recent_artifact_records(MAX_DASHBOARD_ROWS)?;
                let intelligence = controller.recent_intelligence_records(MAX_DASHBOARD_ROWS)?;
                let mut states = HashMap::<String, usize>::new();
                for job in &jobs {
                    *states.entry(job_state_name(job.state).to_owned()).or_default() += 1;
                }
                Ok(json!({
                    "version": env!("CARGO_PKG_VERSION"),
                    "schema_version": controller.schema_version()?,
                    "jobs_visible": jobs.len(),
                    "job_states": states,
                    "workers_total": workers.len(),
                    "workers_online": workers.iter().filter(|worker| worker.online).count(),
                    "plans_total": plans.len(),
                    "artifacts_visible": artifacts.len(),
                    "intelligence_records_visible": intelligence.len(),
                    "audit_chain_verified": controller.verify_audit_chain()?,
                    "read_only": true
                }))
            }
            "/api/jobs" => Ok(serde_json::to_value(
                controller.recent_jobs(MAX_DASHBOARD_ROWS)?,
            )?),
            "/api/workers" => Ok(serde_json::to_value(controller.list_workers()?)?),
            "/api/plans" => {
                let mut plans = Vec::new();
                for name in controller.list_test_plans()? {
                    if let Some(plan) = controller.get_test_plan(&name)? {
                        plans.push(plan);
                    }
                }
                Ok(serde_json::to_value(plans)?)
            }
            "/api/artifacts" => Ok(serde_json::to_value(
                controller.recent_artifact_records(MAX_DASHBOARD_ROWS)?,
            )?),
            "/api/intelligence" => Ok(serde_json::to_value(
                controller.recent_intelligence_records(MAX_DASHBOARD_ROWS)?,
            )?),
            "/api/audit" => Ok(json!({
                "chain_verified": controller.verify_audit_chain()?,
                "events": controller.recent_audit_events(MAX_DASHBOARD_ROWS)?
            })),
            "/api/settings" => Ok(json!({
                "bind": self.config.bind.to_string(),
                "loopback_only": true,
                "authentication": "bearer_sha256_compare",
                "read_only": true,
                "row_limit": MAX_DASHBOARD_ROWS,
                "state_database_configured": true,
                "terminal_access": false,
                "raw_command_access": false,
                "raw_sql_access": false,
                "filesystem_browser": false
            })),
            _ => Ok(json!({"error":"not found"})),
        }
    }
}

fn job_state_name(state: DurableJobState) -> &'static str {
    match state {
        DurableJobState::Queued => "queued",
        DurableJobState::Assigned => "assigned",
        DurableJobState::Running => "running",
        DurableJobState::Passed => "passed",
        DurableJobState::Failed => "failed",
        DurableJobState::Rejected => "rejected",
        DurableJobState::Cancelled => "cancelled",
        DurableJobState::Interrupted => "interrupted",
        DurableJobState::RetryPending => "retry_pending",
        DurableJobState::Exhausted => "exhausted",
    }
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl HttpRequest {
    pub fn get(path: &str, bearer_token: Option<&str>) -> Self {
        let mut headers = HashMap::new();
        if let Some(token) = bearer_token {
            headers.insert("authorization".into(), format!("Bearer {token}"));
        }
        Self {
            method: "GET".into(),
            path: path.into(),
            headers,
            body: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl HttpResponse {
    fn json(status: u16, value: Value) -> Self {
        Self {
            status,
            content_type: "application/json; charset=utf-8",
            body: serde_json::to_vec(&value)
                .unwrap_or_else(|_| b"{\"error\":\"serialization failed\"}".to_vec()),
        }
    }

    fn html(status: u16, body: &'static str) -> Self {
        Self {
            status,
            content_type: "text/html; charset=utf-8",
            body: body.as_bytes().to_vec(),
        }
    }

    fn javascript(status: u16, body: &'static str) -> Self {
        Self {
            status,
            content_type: "text/javascript; charset=utf-8",
            body: body.as_bytes().to_vec(),
        }
    }

    fn css(status: u16, body: &'static str) -> Self {
        Self {
            status,
            content_type: "text/css; charset=utf-8",
            body: body.as_bytes().to_vec(),
        }
    }

    fn unauthorized() -> Self {
        Self::json(401, json!({"error":"unauthorized"}))
    }
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, DashboardError> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 2048];
    let header_end;
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Err(DashboardError::InvalidHttpRequest);
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > MAX_HTTP_HEADER_BYTES + MAX_HTTP_BODY_BYTES {
            return Err(DashboardError::RequestTooLarge);
        }
        if let Some(position) = find_bytes(&bytes, b"\r\n\r\n") {
            header_end = position + 4;
            break;
        }
        if bytes.len() > MAX_HTTP_HEADER_BYTES {
            return Err(DashboardError::RequestTooLarge);
        }
    }

    let header_text =
        std::str::from_utf8(&bytes[..header_end]).map_err(|_| DashboardError::InvalidHttpRequest)?;
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().ok_or(DashboardError::InvalidHttpRequest)?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .ok_or(DashboardError::InvalidHttpRequest)?
        .to_owned();
    let raw_path = request_parts
        .next()
        .ok_or(DashboardError::InvalidHttpRequest)?;
    if request_parts.next() != Some("HTTP/1.1") || request_parts.next().is_some() {
        return Err(DashboardError::InvalidHttpRequest);
    }
    if !raw_path.starts_with('/') || raw_path.len() > 2048 {
        return Err(DashboardError::InvalidHttpRequest);
    }
    let path = raw_path
        .split('?')
        .next()
        .ok_or(DashboardError::InvalidHttpRequest)?
        .to_owned();

    let mut headers = HashMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let (name, value) = line
            .split_once(':')
            .ok_or(DashboardError::InvalidHttpRequest)?;
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
    }
    let content_length = headers
        .get("content-length")
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| DashboardError::InvalidHttpRequest)?
        .unwrap_or(0);
    if content_length > MAX_HTTP_BODY_BYTES {
        return Err(DashboardError::RequestTooLarge);
    }
    while bytes.len() < header_end + content_length {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Err(DashboardError::InvalidHttpRequest);
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    Ok(HttpRequest {
        method,
        path,
        headers,
        body: bytes[header_end..header_end + content_length].to_vec(),
    })
}

fn write_http_response(
    stream: &mut TcpStream,
    response: HttpResponse,
) -> Result<(), DashboardError> {
    let reason = match response.status {
        200 => "OK",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    };
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'\r\nConnection: close\r\n\r\n",
        response.status,
        reason,
        response.content_type,
        response.body.len()
    )?;
    stream.write_all(&response.body)?;
    Ok(())
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn constant_time_equal(left: &[u8; 32], right: &[u8; 32]) -> bool {
    let mut difference = 0u8;
    for index in 0..left.len() {
        difference |= left[index] ^ right[index];
    }
    difference == 0
}

#[derive(Debug, Error)]
pub enum DashboardError {
    #[error("dashboard must bind to a nonzero loopback address")]
    NonLoopbackBind,
    #[error("dashboard bearer token must be 32 to 4096 bytes")]
    InvalidBearerToken,
    #[error("invalid HTTP request")]
    InvalidHttpRequest,
    #[error("HTTP request exceeded configured bounds")]
    RequestTooLarge,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Controller(#[from] df_test_controller::DurableControllerError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashboardFixtureReport {
    pub loopback_enforced: bool,
    pub authentication_enforced: bool,
    pub overview_available: bool,
    pub jobs_available: bool,
    pub workers_available: bool,
    pub plans_available: bool,
    pub artifacts_available: bool,
    pub intelligence_available: bool,
    pub audit_available: bool,
    pub settings_safe: bool,
    pub mutating_methods_rejected: bool,
}

pub fn run_dashboard_fixture() -> Result<DashboardFixtureReport, DashboardError> {
    use df_test_plans::{
        ArtifactKind, PlanCondition, PlanProfile, PlanStep, TargetOs, TestPlan, TEST_PLAN_VERSION,
    };
    use df_test_protocol::{
        Capability, JobRequest, RepositorySpec, ResourceLimits, WorkerRegistration,
        PROTOCOL_VERSION,
    };
    use std::collections::{BTreeMap, BTreeSet};

    let path = std::env::temp_dir().join(format!(
        "dragonforge-phase18-dashboard-{}.sqlite3",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let token = "phase18-dashboard-fixture-token-000000000000";
    let mut controller = DurableController::open(&path)?;
    let job = JobRequest::new(
        RepositorySpec {
            url: "https://github.com/djames1987/DragonForge-Test-Lab.git".into(),
            revision: "main".into(),
        },
        vec![],
    );
    controller.enqueue_job(&job, 1)?;
    controller.register_worker(
        &WorkerRegistration {
            worker_id: "phase18-worker".into(),
            protocol_version: PROTOCOL_VERSION,
            os: "windows".into(),
            arch: "x86_64".into(),
            capabilities: [Capability::CheckoutRepository].into_iter().collect(),
        },
        2,
    )?;
    controller.upsert_test_plan(
        &TestPlan {
            version: TEST_PLAN_VERSION,
            name: "phase18-dashboard".into(),
            repository: RepositorySpec {
                url: "https://github.com/djames1987/DragonForge-Test-Lab.git".into(),
                revision: "main".into(),
            },
            steps: vec![PlanStep {
                id: "observe".into(),
                profile: PlanProfile::RustFast,
                depends_on: vec![],
                condition: PlanCondition::DependenciesPassed,
                limits: ResourceLimits::default(),
                required_capabilities: BTreeSet::new(),
                artifacts: vec![ArtifactKind::ExecutionReport],
                retry: df_test_lifecycle::RetryPolicy::no_retry(),
                target_os: TargetOs::Any,
                node_labels: BTreeMap::new(),
            }],
        },
        3,
    )?;
    controller.record_intelligence(&json!({"failure_clusters":[],"fixture":"phase18"}), 4)?;
    drop(controller);

    let dashboard = Dashboard::new(DashboardConfig {
        bind: DEFAULT_DASHBOARD_BIND
            .parse()
            .map_err(|_| DashboardError::InvalidHttpRequest)?,
        bearer_token: token.into(),
        state_db: path.clone(),
    })?;
    let non_loopback = Dashboard::new(DashboardConfig {
        bind: "0.0.0.0:8788"
            .parse()
            .map_err(|_| DashboardError::InvalidHttpRequest)?,
        bearer_token: token.into(),
        state_db: path.clone(),
    });

    let authentication_enforced =
        dashboard.handle_request(HttpRequest::get("/api/overview", None)).status == 401;
    let overview_available =
        dashboard.handle_request(HttpRequest::get("/api/overview", Some(token))).status == 200;
    let jobs_available =
        dashboard.handle_request(HttpRequest::get("/api/jobs", Some(token))).status == 200;
    let workers_available =
        dashboard.handle_request(HttpRequest::get("/api/workers", Some(token))).status == 200;
    let plans_available =
        dashboard.handle_request(HttpRequest::get("/api/plans", Some(token))).status == 200;
    let artifacts_available =
        dashboard.handle_request(HttpRequest::get("/api/artifacts", Some(token))).status == 200;
    let intelligence_available =
        dashboard.handle_request(HttpRequest::get("/api/intelligence", Some(token))).status == 200;
    let audit_available =
        dashboard.handle_request(HttpRequest::get("/api/audit", Some(token))).status == 200;
    let settings = dashboard.handle_request(HttpRequest::get("/api/settings", Some(token)));
    let settings_text = String::from_utf8_lossy(&settings.body);
    let settings_safe = settings.status == 200
        && settings_text.contains("\"terminal_access\":false")
        && settings_text.contains("\"raw_command_access\":false")
        && !settings_text.contains(token);
    let mutating_methods_rejected = dashboard
        .handle_request(HttpRequest {
            method: "POST".into(),
            path: "/api/jobs".into(),
            headers: HashMap::new(),
            body: Vec::new(),
        })
        .status
        == 405;

    let report = DashboardFixtureReport {
        loopback_enforced: matches!(non_loopback, Err(DashboardError::NonLoopbackBind)),
        authentication_enforced,
        overview_available,
        jobs_available,
        workers_available,
        plans_available,
        artifacts_available,
        intelligence_available,
        audit_available,
        settings_safe,
        mutating_methods_rejected,
    };
    let _ = std::fs::remove_file(path);
    Ok(report)
}

const DASHBOARD_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>DragonForge Test Lab</title>
<link rel="stylesheet" href="/dashboard.css">
</head>
<body>
<header><div><h1>DragonForge Test Lab</h1><p>Local operator dashboard · read-only control plane view</p></div><span id="status">Connecting…</span></header>
<nav>
<button data-view="overview">Overview</button><button data-view="jobs">Jobs</button>
<button data-view="workers">Workers</button><button data-view="plans">Plans</button>
<button data-view="artifacts">Artifacts</button><button data-view="intelligence">Intelligence</button>
<button data-view="audit">Audit</button><button data-view="settings">Settings</button>
</nav>
<main><section id="content"><p>Provide the dashboard token in the URL fragment as <code>#token=...</code>.</p></section></main>
<script src="/dashboard.js"></script>
</body>
</html>"#;

const DASHBOARD_JS: &str = r#"const content=document.getElementById('content');const statusEl=document.getElementById('status');
const fragment=new URLSearchParams(location.hash.slice(1));if(fragment.get('token')){sessionStorage.setItem('dfDashboardToken',fragment.get('token'));history.replaceState(null,'',location.pathname);}
const token=sessionStorage.getItem('dfDashboardToken')||'';
async function load(view){statusEl.textContent='Loading…';try{const r=await fetch('/api/'+view,{headers:{Authorization:'Bearer '+token}});if(!r.ok)throw new Error('HTTP '+r.status);const data=await r.json();statusEl.textContent='Connected · read-only';content.innerHTML='<h2>'+view[0].toUpperCase()+view.slice(1)+'</h2><pre></pre>';content.querySelector('pre').textContent=JSON.stringify(data,null,2);}catch(e){statusEl.textContent='Authentication/query failed';content.innerHTML='<h2>Dashboard unavailable</h2><p>'+String(e)+'</p>';}}
document.querySelectorAll('button[data-view]').forEach(b=>b.addEventListener('click',()=>load(b.dataset.view)));if(token)load('overview');"#;

const DASHBOARD_CSS: &str = r#"*{box-sizing:border-box}body{margin:0;background:#0b1020;color:#e7edf8;font:14px system-ui,sans-serif}header{display:flex;justify-content:space-between;align-items:center;padding:24px 30px;border-bottom:1px solid #26324d}h1{margin:0;font-size:24px}header p{margin:5px 0 0;color:#97a6c4}#status{padding:8px 12px;border:1px solid #334462;border-radius:8px}nav{display:flex;gap:8px;flex-wrap:wrap;padding:16px 30px;border-bottom:1px solid #26324d}button{background:#151f36;color:#e7edf8;border:1px solid #334462;border-radius:7px;padding:9px 13px;cursor:pointer}button:hover{background:#1d2b49}main{padding:24px 30px}section{max-width:1200px}pre{white-space:pre-wrap;overflow-wrap:anywhere;background:#10182a;border:1px solid #26324d;border-radius:10px;padding:18px;line-height:1.45}code{color:#9fc2ff}"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_fixture_covers_security_and_all_views() {
        let report = run_dashboard_fixture().unwrap();
        assert!(report.loopback_enforced);
        assert!(report.authentication_enforced);
        assert!(report.overview_available);
        assert!(report.jobs_available);
        assert!(report.workers_available);
        assert!(report.plans_available);
        assert!(report.artifacts_available);
        assert!(report.intelligence_available);
        assert!(report.audit_available);
        assert!(report.settings_safe);
        assert!(report.mutating_methods_rejected);
    }

    #[test]
    fn wrong_bearer_token_is_rejected() {
        let path = std::env::temp_dir().join("dragonforge-phase18-wrong-token.sqlite3");
        let _ = std::fs::remove_file(&path);
        DurableController::open(&path).unwrap();
        let dashboard = Dashboard::new(DashboardConfig {
            bind: DEFAULT_DASHBOARD_BIND.parse().unwrap(),
            bearer_token: "correct-dashboard-token-000000000000000".into(),
            state_db: path.clone(),
        })
        .unwrap();
        assert_eq!(
            dashboard
                .handle_request(HttpRequest::get(
                    "/api/overview",
                    Some("wrong-dashboard-token-00000000000000000")
                ))
                .status,
            401
        );
        let _ = std::fs::remove_file(path);
    }
}
