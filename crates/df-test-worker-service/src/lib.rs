use df_test_identity::{
    build_client_config, build_server_config, validate_private_controller_address,
    CertificateMaterial, IdentityError,
};
use df_test_protocol::PROTOCOL_VERSION;
use rustls::{pki_types::ServerName, ClientConfig, ClientConnection, StreamOwned};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use thiserror::Error;

pub const MAX_SERVICE_FRAME_BYTES: usize = 256 * 1024;
pub const MIN_HEARTBEAT_SECONDS: u64 = 5;
pub const MAX_HEARTBEAT_SECONDS: u64 = 300;
pub const MAX_PARALLEL_JOBS: u16 = 128;
pub const WINDOWS_SERVICE_NAME: &str = "DragonForgeTestWorker";
pub const SYSTEMD_SERVICE_NAME: &str = "dragonforge-test-worker.service";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerServiceConfig {
    pub worker_id: String,
    pub controller: SocketAddr,
    pub controller_server_name: String,
    pub heartbeat_seconds: u64,
    pub max_parallel_jobs: u16,
    pub state_path: PathBuf,
    pub ca_cert_path: PathBuf,
    pub client_cert_path: PathBuf,
    pub client_key_path: PathBuf,
}

impl WorkerServiceConfig {
    pub fn validate(&self) -> Result<(), WorkerServiceError> {
        validate_identifier(&self.worker_id)?;
        validate_private_controller_address(self.controller)?;
        validate_server_name(&self.controller_server_name)?;
        if !(MIN_HEARTBEAT_SECONDS..=MAX_HEARTBEAT_SECONDS).contains(&self.heartbeat_seconds) {
            return Err(WorkerServiceError::InvalidHeartbeatInterval);
        }
        if !(1..=MAX_PARALLEL_JOBS).contains(&self.max_parallel_jobs) {
            return Err(WorkerServiceError::InvalidParallelism);
        }
        validate_file_path(&self.state_path)?;
        validate_file_path(&self.ca_cert_path)?;
        validate_file_path(&self.client_cert_path)?;
        validate_file_path(&self.client_key_path)?;
        Ok(())
    }

    pub fn load_client_config(&self) -> Result<Arc<ClientConfig>, WorkerServiceError> {
        self.validate()?;
        let ca = fs::read(&self.ca_cert_path)?;
        let certificate = fs::read(&self.client_cert_path)?;
        let key = fs::read(&self.client_key_path)?;
        let material = CertificateMaterial::from_pem(&certificate, &key)?;
        Ok(Arc::new(build_client_config(&ca, material)?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerServiceState {
    Starting,
    Connecting,
    Online,
    Draining,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerRuntimeSnapshot {
    pub worker_id: String,
    pub state: WorkerServiceState,
    pub active_jobs: u16,
    pub drain_requested: bool,
    pub last_heartbeat_secs: Option<u64>,
    pub reconnect_attempt: u32,
}

impl WorkerRuntimeSnapshot {
    pub fn validate(&self, max_parallel_jobs: u16) -> Result<(), WorkerServiceError> {
        validate_identifier(&self.worker_id)?;
        if self.active_jobs > max_parallel_jobs {
            return Err(WorkerServiceError::ActiveJobsExceedCapacity);
        }
        if self.reconnect_attempt > 1_000_000 {
            return Err(WorkerServiceError::InvalidRuntimeState);
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct WorkerServiceRuntime {
    config: WorkerServiceConfig,
    snapshot: WorkerRuntimeSnapshot,
}

impl WorkerServiceRuntime {
    pub fn new(config: WorkerServiceConfig) -> Result<Self, WorkerServiceError> {
        config.validate()?;
        let snapshot = WorkerRuntimeSnapshot {
            worker_id: config.worker_id.clone(),
            state: WorkerServiceState::Starting,
            active_jobs: 0,
            drain_requested: false,
            last_heartbeat_secs: None,
            reconnect_attempt: 0,
        };
        Ok(Self { config, snapshot })
    }

    pub fn recover(config: WorkerServiceConfig) -> Result<Self, WorkerServiceError> {
        config.validate()?;
        if !config.state_path.exists() {
            return Self::new(config);
        }
        let json = fs::read_to_string(&config.state_path)?;
        let mut snapshot: WorkerRuntimeSnapshot = serde_json::from_str(&json)?;
        snapshot.validate(config.max_parallel_jobs)?;
        if snapshot.worker_id != config.worker_id {
            return Err(WorkerServiceError::StateIdentityMismatch);
        }
        snapshot.state = if snapshot.drain_requested {
            WorkerServiceState::Draining
        } else {
            WorkerServiceState::Connecting
        };
        snapshot.last_heartbeat_secs = None;
        Ok(Self { config, snapshot })
    }

    pub fn snapshot(&self) -> &WorkerRuntimeSnapshot {
        &self.snapshot
    }

    pub fn config(&self) -> &WorkerServiceConfig {
        &self.config
    }

    pub fn mark_connecting(&mut self) {
        self.snapshot.state = WorkerServiceState::Connecting;
    }

    pub fn mark_connected(&mut self, now_secs: u64) {
        self.snapshot.state = if self.snapshot.drain_requested {
            WorkerServiceState::Draining
        } else {
            WorkerServiceState::Online
        };
        self.snapshot.reconnect_attempt = 0;
        self.snapshot.last_heartbeat_secs = Some(now_secs);
    }

    pub fn mark_disconnected(&mut self) {
        self.snapshot.state = WorkerServiceState::Offline;
        self.snapshot.last_heartbeat_secs = None;
        self.snapshot.reconnect_attempt = self.snapshot.reconnect_attempt.saturating_add(1);
    }

    pub fn request_drain(&mut self) {
        self.snapshot.drain_requested = true;
        self.snapshot.state = WorkerServiceState::Draining;
    }

    pub fn cancel_drain(&mut self) {
        self.snapshot.drain_requested = false;
        self.snapshot.state = WorkerServiceState::Connecting;
    }

    pub fn can_accept_job(&self) -> bool {
        self.snapshot.state == WorkerServiceState::Online
            && !self.snapshot.drain_requested
            && self.snapshot.active_jobs < self.config.max_parallel_jobs
    }

    pub fn start_job(&mut self) -> Result<(), WorkerServiceError> {
        if !self.can_accept_job() {
            return Err(WorkerServiceError::WorkerNotAcceptingJobs);
        }
        self.snapshot.active_jobs += 1;
        Ok(())
    }

    pub fn finish_job(&mut self) -> Result<(), WorkerServiceError> {
        if self.snapshot.active_jobs == 0 {
            return Err(WorkerServiceError::NoActiveJob);
        }
        self.snapshot.active_jobs -= 1;
        Ok(())
    }

    pub fn heartbeat_due(&self, now_secs: u64) -> bool {
        match self.snapshot.last_heartbeat_secs {
            None => true,
            Some(last) => now_secs.saturating_sub(last) >= self.config.heartbeat_seconds,
        }
    }

    pub fn mark_heartbeat_sent(&mut self, now_secs: u64) {
        self.snapshot.last_heartbeat_secs = Some(now_secs);
    }

    pub fn reconnect_delay(&self) -> Duration {
        let exponent = self.snapshot.reconnect_attempt.min(6);
        Duration::from_secs(1u64 << exponent)
    }

    pub fn persist(&self) -> Result<(), WorkerServiceError> {
        self.snapshot.validate(self.config.max_parallel_jobs)?;
        if let Some(parent) = self.config.state_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp_path = self.config.state_path.with_extension("tmp");
        let json = serde_json::to_vec_pretty(&self.snapshot)?;
        fs::write(&temp_path, json)?;
        if self.config.state_path.exists() {
            fs::remove_file(&self.config.state_path)?;
        }
        fs::rename(temp_path, &self.config.state_path)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerServiceHello {
    pub protocol_version: u16,
    pub worker_id: String,
    pub max_parallel_jobs: u16,
    pub drain_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerHeartbeat {
    pub protocol_version: u16,
    pub worker_id: String,
    pub active_jobs: u16,
    pub accepting_jobs: bool,
    pub draining: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControllerServiceAck {
    pub accepted: bool,
    pub worker_id: String,
    pub heartbeat_seconds: u64,
}

pub struct MtlsWorkerSession {
    stream: StreamOwned<ClientConnection, TcpStream>,
}

impl MtlsWorkerSession {
    pub fn connect(
        config: &WorkerServiceConfig,
        client_config: Arc<ClientConfig>,
        timeout: Duration,
    ) -> Result<Self, WorkerServiceError> {
        config.validate()?;
        if timeout.is_zero() || timeout > Duration::from_secs(30) {
            return Err(WorkerServiceError::InvalidConnectTimeout);
        }
        let tcp = TcpStream::connect_timeout(&config.controller, timeout)?;
        tcp.set_read_timeout(Some(timeout))?;
        tcp.set_write_timeout(Some(timeout))?;
        let server_name = ServerName::try_from(config.controller_server_name.clone())
            .map_err(|_| WorkerServiceError::InvalidServerName)?;
        let connection = ClientConnection::new(client_config, server_name)
            .map_err(|error| WorkerServiceError::Tls(error.to_string()))?;
        Ok(Self {
            stream: StreamOwned::new(connection, tcp),
        })
    }

    pub fn register(
        &mut self,
        runtime: &WorkerServiceRuntime,
    ) -> Result<ControllerServiceAck, WorkerServiceError> {
        let hello = WorkerServiceHello {
            protocol_version: PROTOCOL_VERSION,
            worker_id: runtime.snapshot.worker_id.clone(),
            max_parallel_jobs: runtime.config.max_parallel_jobs,
            drain_requested: runtime.snapshot.drain_requested,
        };
        write_frame(&mut self.stream, &hello)?;
        let ack: ControllerServiceAck = read_frame(&mut self.stream)?;
        if !ack.accepted
            || ack.worker_id != runtime.snapshot.worker_id
            || ack.heartbeat_seconds != runtime.config.heartbeat_seconds
        {
            return Err(WorkerServiceError::RegistrationRejected);
        }
        Ok(ack)
    }

    pub fn send_heartbeat(
        &mut self,
        runtime: &WorkerServiceRuntime,
    ) -> Result<(), WorkerServiceError> {
        let heartbeat = WorkerHeartbeat {
            protocol_version: PROTOCOL_VERSION,
            worker_id: runtime.snapshot.worker_id.clone(),
            active_jobs: runtime.snapshot.active_jobs,
            accepting_jobs: runtime.can_accept_job(),
            draining: runtime.snapshot.drain_requested,
        };
        write_frame(&mut self.stream, &heartbeat)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsServiceSpec {
    pub service_name: String,
    pub display_name: String,
    pub start_type: String,
    pub restart_delays_seconds: Vec<u64>,
    pub executable: PathBuf,
    pub config_path: PathBuf,
}

impl WindowsServiceSpec {
    pub fn new(executable: PathBuf, config_path: PathBuf) -> Result<Self, WorkerServiceError> {
        validate_absolute_executable(&executable)?;
        validate_file_path(&config_path)?;
        Ok(Self {
            service_name: WINDOWS_SERVICE_NAME.to_owned(),
            display_name: "DragonForge Test Worker".to_owned(),
            start_type: "automatic".to_owned(),
            restart_delays_seconds: vec![5, 15, 60],
            executable,
            config_path,
        })
    }

    pub fn command_line(&self) -> String {
        format!(
            "\"{}\" worker-service-run --config \"{}\" --service-mode",
            self.executable.display(),
            self.config_path.display()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemdServiceSpec {
    pub unit_name: String,
    pub executable: PathBuf,
    pub config_path: PathBuf,
}

impl SystemdServiceSpec {
    pub fn new(executable: PathBuf, config_path: PathBuf) -> Result<Self, WorkerServiceError> {
        validate_absolute_executable(&executable)?;
        validate_file_path(&config_path)?;
        Ok(Self {
            unit_name: SYSTEMD_SERVICE_NAME.to_owned(),
            executable,
            config_path,
        })
    }

    pub fn render_unit(&self) -> String {
        format!(
            "[Unit]\nDescription=DragonForge Test Worker\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nExecStart={} worker-service-run --config {} --service-mode\nRestart=on-failure\nRestartSec=5\nNoNewPrivileges=true\nPrivateTmp=true\nProtectSystem=strict\nProtectHome=true\n\n[Install]\nWantedBy=multi-user.target\n",
            systemd_escape_path(&self.executable),
            systemd_escape_path(&self.config_path)
        )
    }
}


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerServiceFixtureReport {
    pub mtls_registration: bool,
    pub heartbeat_received: bool,
    pub drain_blocks_new_jobs: bool,
    pub restart_state_recovered: bool,
    pub windows_service_spec_valid: bool,
    pub systemd_unit_valid: bool,
}

pub fn run_worker_service_fixture() -> Result<WorkerServiceFixtureReport, WorkerServiceError> {
    use rcgen::{BasicConstraints, CertificateParams, CertifiedIssuer, IsCa, KeyPair};

    let mut ca_params = CertificateParams::new(Vec::<String>::new())
        .map_err(|error| WorkerServiceError::CertificateFixture(error.to_string()))?;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca_key =
        KeyPair::generate().map_err(|error| WorkerServiceError::CertificateFixture(error.to_string()))?;
    let ca = CertifiedIssuer::self_signed(ca_params, ca_key)
        .map_err(|error| WorkerServiceError::CertificateFixture(error.to_string()))?;

    let server_key =
        KeyPair::generate().map_err(|error| WorkerServiceError::CertificateFixture(error.to_string()))?;
    let server_params = CertificateParams::new(vec!["localhost".to_owned()])
        .map_err(|error| WorkerServiceError::CertificateFixture(error.to_string()))?;
    let server_cert = server_params
        .signed_by(&server_key, &ca)
        .map_err(|error| WorkerServiceError::CertificateFixture(error.to_string()))?;

    let client_key =
        KeyPair::generate().map_err(|error| WorkerServiceError::CertificateFixture(error.to_string()))?;
    let client_params = CertificateParams::new(Vec::<String>::new())
        .map_err(|error| WorkerServiceError::CertificateFixture(error.to_string()))?;
    let client_cert = client_params
        .signed_by(&client_key, &ca)
        .map_err(|error| WorkerServiceError::CertificateFixture(error.to_string()))?;

    let ca_pem = ca.pem();
    let server_material = CertificateMaterial::from_pem(
        server_cert.pem().as_bytes(),
        server_key.serialize_pem().as_bytes(),
    )?;
    let client_material = CertificateMaterial::from_pem(
        client_cert.pem().as_bytes(),
        client_key.serialize_pem().as_bytes(),
    )?;
    let server_config = Arc::new(build_server_config(ca_pem.as_bytes(), server_material)?);
    let client_config = Arc::new(build_client_config(ca_pem.as_bytes(), client_material)?);

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let controller = listener.local_addr()?;
    let server = std::thread::spawn(move || -> Result<bool, String> {
        let (tcp, _) = listener.accept().map_err(|error| error.to_string())?;
        tcp.set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|error| error.to_string())?;
        tcp.set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(|error| error.to_string())?;
        let connection =
            rustls::ServerConnection::new(server_config).map_err(|error| error.to_string())?;
        let mut stream = StreamOwned::new(connection, tcp);
        let hello: WorkerServiceHello =
            read_frame(&mut stream).map_err(|error| error.to_string())?;
        if hello.protocol_version != PROTOCOL_VERSION || hello.worker_id != "fixture-worker" {
            return Err("unexpected worker service hello".to_owned());
        }
        let ack = ControllerServiceAck {
            accepted: true,
            worker_id: hello.worker_id.clone(),
            heartbeat_seconds: 5,
        };
        write_frame(&mut stream, &ack).map_err(|error| error.to_string())?;
        let heartbeat: WorkerHeartbeat =
            read_frame(&mut stream).map_err(|error| error.to_string())?;
        Ok(heartbeat.worker_id == hello.worker_id && heartbeat.accepting_jobs)
    });

    let state_path = std::env::temp_dir().join(format!(
        "dragonforge-worker-service-fixture-{}.json",
        std::process::id()
    ));
    let _ = fs::remove_file(&state_path);
    let config = WorkerServiceConfig {
        worker_id: "fixture-worker".into(),
        controller,
        controller_server_name: "localhost".into(),
        heartbeat_seconds: 5,
        max_parallel_jobs: 2,
        state_path: state_path.clone(),
        ca_cert_path: PathBuf::from("fixture-ca.pem"),
        client_cert_path: PathBuf::from("fixture-worker.pem"),
        client_key_path: PathBuf::from("fixture-worker-key.pem"),
    };

    let mut runtime = WorkerServiceRuntime::new(config.clone())?;
    runtime.mark_connecting();
    let mut session =
        MtlsWorkerSession::connect(&config, client_config, Duration::from_secs(5))?;
    let ack = session.register(&runtime)?;
    let mtls_registration = ack.accepted && ack.worker_id == config.worker_id;
    runtime.mark_connected(100);
    session.send_heartbeat(&runtime)?;
    let heartbeat_received = server
        .join()
        .map_err(|_| WorkerServiceError::FixtureThreadPanicked)?
        .map_err(WorkerServiceError::Fixture)?;

    runtime.start_job()?;
    runtime.request_drain();
    let drain_blocks_new_jobs = !runtime.can_accept_job() && runtime.snapshot.active_jobs == 1;
    runtime.persist()?;
    let recovered = WorkerServiceRuntime::recover(config)?;
    let restart_state_recovered = recovered.snapshot.state == WorkerServiceState::Draining
        && recovered.snapshot.active_jobs == 1
        && recovered.snapshot.drain_requested;
    let _ = fs::remove_file(state_path);

    let executable = if cfg!(windows) {
        PathBuf::from(r"C:\Program Files\DragonForge\Test Lab\dragonforge-test-lab.exe")
    } else {
        PathBuf::from("/opt/dragonforge/bin/dragonforge-test-lab")
    };
    let windows = WindowsServiceSpec::new(executable, PathBuf::from("worker.json"))?;
    let windows_service_spec_valid = windows.service_name == WINDOWS_SERVICE_NAME
        && windows.start_type == "automatic"
        && windows.command_line().contains("worker-service-windows");

    let systemd = SystemdServiceSpec::new(
        PathBuf::from("/opt/dragonforge/bin/dragonforge-test-lab"),
        PathBuf::from("/etc/dragonforge/test-worker.json"),
    )?;
    let unit = systemd.render_unit();
    let systemd_unit_valid = unit.contains("Restart=on-failure")
        && unit.contains("NoNewPrivileges=true")
        && unit.contains("worker-service-run");

    Ok(WorkerServiceFixtureReport {
        mtls_registration,
        heartbeat_received,
        drain_blocks_new_jobs,
        restart_state_recovered,
        windows_service_spec_valid,
        systemd_unit_valid,
    })
}

pub fn load_worker_config(path: impl AsRef<Path>) -> Result<WorkerServiceConfig, WorkerServiceError> {
    let content = fs::read_to_string(path)?;
    let config: WorkerServiceConfig = serde_json::from_str(&content)?;
    config.validate()?;
    Ok(config)
}

pub fn save_worker_config(
    path: impl AsRef<Path>,
    config: &WorkerServiceConfig,
) -> Result<(), WorkerServiceError> {
    config.validate()?;
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(config)?)?;
    Ok(())
}

fn write_frame<T: Serialize, W: Write>(
    writer: &mut W,
    value: &T,
) -> Result<(), WorkerServiceError> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.is_empty() || bytes.len() > MAX_SERVICE_FRAME_BYTES {
        return Err(WorkerServiceError::FrameTooLarge);
    }
    writer.write_all(&(bytes.len() as u32).to_be_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()?;
    Ok(())
}

fn read_frame<T: DeserializeOwned, R: Read>(reader: &mut R) -> Result<T, WorkerServiceError> {
    let mut length = [0u8; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_SERVICE_FRAME_BYTES {
        return Err(WorkerServiceError::FrameTooLarge);
    }
    let mut bytes = vec![0u8; length];
    reader.read_exact(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn validate_identifier(value: &str) -> Result<(), WorkerServiceError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(WorkerServiceError::InvalidWorkerId);
    }
    Ok(())
}

fn validate_server_name(value: &str) -> Result<(), WorkerServiceError> {
    if value.is_empty()
        || value.len() > 253
        || value.contains(char::is_whitespace)
        || value.contains('/')
        || value.contains('\\')
    {
        return Err(WorkerServiceError::InvalidServerName);
    }
    Ok(())
}

fn validate_file_path(path: &Path) -> Result<(), WorkerServiceError> {
    let text = path.to_string_lossy();
    if text.is_empty()
        || text.len() > 2048
        || text.chars().any(|ch| ch == '\r' || ch == '\n' || ch == '\0')
    {
        return Err(WorkerServiceError::InvalidPath);
    }
    Ok(())
}

fn validate_absolute_executable(path: &Path) -> Result<(), WorkerServiceError> {
    validate_file_path(path)?;
    let text = path.to_string_lossy();
    let windows_drive_absolute = text.len() >= 3
        && text.as_bytes()[1] == b':'
        && matches!(text.as_bytes()[2], b'\\' | b'/')
        && text.as_bytes()[0].is_ascii_alphabetic();
    let windows_unc_absolute = text.starts_with("\\\\");
    let unix_absolute = text.starts_with('/');
    if !path.is_absolute() && !windows_drive_absolute && !windows_unc_absolute && !unix_absolute {
        return Err(WorkerServiceError::ExecutableMustBeAbsolute);
    }
    Ok(())
}

fn systemd_escape_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[derive(Debug, Error)]
pub enum WorkerServiceError {
    #[error("invalid worker id")]
    InvalidWorkerId,
    #[error("invalid heartbeat interval")]
    InvalidHeartbeatInterval,
    #[error("invalid worker parallelism")]
    InvalidParallelism,
    #[error("invalid controller TLS server name")]
    InvalidServerName,
    #[error("invalid filesystem path")]
    InvalidPath,
    #[error("service executable path must be absolute")]
    ExecutableMustBeAbsolute,
    #[error("active jobs exceed worker capacity")]
    ActiveJobsExceedCapacity,
    #[error("persisted runtime state is invalid")]
    InvalidRuntimeState,
    #[error("persisted runtime state belongs to a different worker")]
    StateIdentityMismatch,
    #[error("worker is draining, offline, or at capacity")]
    WorkerNotAcceptingJobs,
    #[error("worker has no active job to finish")]
    NoActiveJob,
    #[error("invalid controller connect timeout")]
    InvalidConnectTimeout,
    #[error("controller rejected worker registration")]
    RegistrationRejected,
    #[error("service frame is invalid or too large")]
    FrameTooLarge,
    #[error("TLS error: {0}")]
    Tls(String),
    #[error("certificate fixture error: {0}")]
    CertificateFixture(String),
    #[error("worker service fixture error: {0}")]
    Fixture(String),
    #[error("worker service fixture thread panicked")]
    FixtureThreadPanicked,
    #[error("identity error: {0}")]
    Identity(#[from] IdentityError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn config(state_path: PathBuf) -> WorkerServiceConfig {
        WorkerServiceConfig {
            worker_id: "worker-01".into(),
            controller: "127.0.0.1:45900".parse().unwrap(),
            controller_server_name: "localhost".into(),
            heartbeat_seconds: 15,
            max_parallel_jobs: 2,
            state_path,
            ca_cert_path: PathBuf::from("certs/ca.pem"),
            client_cert_path: PathBuf::from("certs/worker.pem"),
            client_key_path: PathBuf::from("certs/worker-key.pem"),
        }
    }

    #[test]
    fn drain_stops_new_work_and_allows_existing_work_to_finish() {
        let path = env::temp_dir().join("dragonforge-worker-drain.json");
        let mut runtime = WorkerServiceRuntime::new(config(path)).unwrap();
        runtime.mark_connected(100);
        runtime.start_job().unwrap();
        runtime.request_drain();
        assert!(!runtime.can_accept_job());
        assert_eq!(runtime.snapshot().active_jobs, 1);
        runtime.finish_job().unwrap();
        assert_eq!(runtime.snapshot().active_jobs, 0);
        assert_eq!(runtime.snapshot().state, WorkerServiceState::Draining);
    }

    #[test]
    fn runtime_state_survives_restart_without_claiming_online() {
        let path = env::temp_dir().join(format!(
            "dragonforge-worker-state-{}.json",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        let cfg = config(path.clone());
        {
            let mut runtime = WorkerServiceRuntime::new(cfg.clone()).unwrap();
            runtime.mark_connected(100);
            runtime.start_job().unwrap();
            runtime.persist().unwrap();
        }
        let recovered = WorkerServiceRuntime::recover(cfg).unwrap();
        assert_eq!(recovered.snapshot().state, WorkerServiceState::Connecting);
        assert_eq!(recovered.snapshot().active_jobs, 1);
        assert!(!recovered.can_accept_job());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn reconnect_backoff_is_bounded() {
        let path = env::temp_dir().join("dragonforge-worker-backoff.json");
        let mut runtime = WorkerServiceRuntime::new(config(path)).unwrap();
        for _ in 0..20 {
            runtime.mark_disconnected();
        }
        assert_eq!(runtime.reconnect_delay(), Duration::from_secs(64));
    }

    #[test]
    fn windows_service_spec_is_fixed_and_automatic() {
        let exe = if cfg!(windows) {
            PathBuf::from(r"C:\Program Files\DragonForge\Test Lab\dragonforge-test-lab.exe")
        } else {
            PathBuf::from("/opt/dragonforge/bin/dragonforge-test-lab")
        };
        let spec = WindowsServiceSpec::new(exe, PathBuf::from("worker.json")).unwrap();
        assert_eq!(spec.service_name, WINDOWS_SERVICE_NAME);
        assert_eq!(spec.start_type, "automatic");
        assert_eq!(spec.restart_delays_seconds, vec![5, 15, 60]);
        assert!(spec.command_line().contains("worker-service-windows"));
    }

    #[test]
    fn systemd_unit_has_hardening_and_restart_policy() {
        let spec = SystemdServiceSpec::new(
            PathBuf::from("/opt/dragonforge/bin/dragonforge-test-lab"),
            PathBuf::from("/etc/dragonforge/test-worker.json"),
        )
        .unwrap();
        let unit = spec.render_unit();
        assert!(unit.contains("Restart=on-failure"));
        assert!(unit.contains("NoNewPrivileges=true"));
        assert!(unit.contains("ProtectSystem=strict"));
        assert!(unit.contains("worker-service-run"));
    }

    #[test]
    fn real_worker_service_fixture_passes() {
        let report = run_worker_service_fixture().unwrap();
        assert!(report.mtls_registration);
        assert!(report.heartbeat_received);
        assert!(report.drain_blocks_new_jobs);
        assert!(report.restart_state_recovered);
        assert!(report.windows_service_spec_valid);
        assert!(report.systemd_unit_valid);
    }

    #[test]
    fn config_rejects_public_controller_addresses() {
        let path = env::temp_dir().join("dragonforge-worker-public.json");
        let mut cfg = config(path);
        cfg.controller = "8.8.8.8:443".parse().unwrap();
        assert!(cfg.validate().is_err());
    }
}
