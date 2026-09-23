use df_test_protocol::{JobStatus, PROTOCOL_VERSION};
use hmac::{Hmac, Mac};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    io::{Read, Write},
    net::{
        IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket,
    },
    time::{Duration, Instant},
};
use thiserror::Error;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

pub const DEFAULT_LEASE_SECONDS: u64 = 30;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeFeature {
    Rust,
    Docker,
    Podman,
    HyperV,
    WindowsIntegration,
    GuiAutomation,
    TcpFixture,
    UdpFixture,
    DnsFixture,
    FaultInjection,
    HardwareIo,
    ArmWorker,
    RaspberryPi,
    Gpio,
    I2c,
    Spi,
    Uart,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeProfile {
    pub node_id: String,
    pub os: String,
    pub arch: String,
    pub labels: BTreeSet<String>,
    pub features: BTreeSet<NodeFeature>,
    pub max_parallel_jobs: u16,
}

impl NodeProfile {
    pub fn validate(&self) -> Result<(), DistributedError> {
        validate_identifier(&self.node_id, 64)?;
        validate_token(&self.os, 32)?;
        validate_token(&self.arch, 32)?;
        if !(1..=128).contains(&self.max_parallel_jobs) {
            return Err(DistributedError::InvalidParallelism);
        }
        if self.labels.len() > 64 {
            return Err(DistributedError::TooManyLabels);
        }
        for label in &self.labels {
            validate_identifier(label, 64)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeRegistration {
    pub protocol_version: u16,
    pub profile: NodeProfile,
    pub outbound_only: bool,
    pub key_id: String,
}

impl NodeRegistration {
    pub fn validate(&self) -> Result<(), DistributedError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(DistributedError::ProtocolMismatch);
        }
        if !self.outbound_only {
            return Err(DistributedError::InboundAgentListenerForbidden);
        }
        validate_identifier(&self.key_id, 64)?;
        self.profile.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeHeartbeat {
    pub node_id: String,
    pub load_percent: u8,
    pub active_jobs: u16,
}

impl NodeHeartbeat {
    pub fn validate(&self) -> Result<(), DistributedError> {
        validate_identifier(&self.node_id, 64)?;
        if self.load_percent > 100 {
            return Err(DistributedError::InvalidLoad);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthenticatedEnvelope<T> {
    pub key_id: String,
    pub nonce: String,
    pub issued_at_secs: u64,
    pub payload: T,
    pub mac_hex: String,
}

impl<T: Serialize> AuthenticatedEnvelope<T> {
    pub fn sign(
        key_id: impl Into<String>,
        secret: &[u8],
        issued_at_secs: u64,
        payload: T,
    ) -> Result<Self, DistributedError> {
        let key_id = key_id.into();
        validate_identifier(&key_id, 64)?;
        if secret.len() < 32 {
            return Err(DistributedError::WeakSharedSecret);
        }
        let nonce = Uuid::new_v4().simple().to_string();
        let mac_hex = compute_mac(&key_id, &nonce, issued_at_secs, &payload, secret)?;
        Ok(Self {
            key_id,
            nonce,
            issued_at_secs,
            payload,
            mac_hex,
        })
    }
}

#[derive(Debug, Clone)]
pub struct EnvelopeVerifier {
    keys: BTreeMap<String, Vec<u8>>,
    seen_nonces: HashMap<String, u64>,
    max_clock_skew_secs: u64,
}

impl EnvelopeVerifier {
    pub fn new(max_clock_skew_secs: u64) -> Result<Self, DistributedError> {
        if !(1..=300).contains(&max_clock_skew_secs) {
            return Err(DistributedError::InvalidClockSkew);
        }
        Ok(Self {
            keys: BTreeMap::new(),
            seen_nonces: HashMap::new(),
            max_clock_skew_secs,
        })
    }

    pub fn add_key(
        &mut self,
        key_id: impl Into<String>,
        secret: &[u8],
    ) -> Result<(), DistributedError> {
        let key_id = key_id.into();
        validate_identifier(&key_id, 64)?;
        if secret.len() < 32 {
            return Err(DistributedError::WeakSharedSecret);
        }
        self.keys.insert(key_id, secret.to_vec());
        Ok(())
    }

    pub fn verify<T: Serialize>(
        &mut self,
        envelope: &AuthenticatedEnvelope<T>,
        now_secs: u64,
    ) -> Result<(), DistributedError> {
        if now_secs.abs_diff(envelope.issued_at_secs) > self.max_clock_skew_secs {
            return Err(DistributedError::StaleEnvelope);
        }
        if self.seen_nonces.contains_key(&envelope.nonce) {
            return Err(DistributedError::ReplayDetected);
        }
        let secret = self
            .keys
            .get(&envelope.key_id)
            .ok_or(DistributedError::UnknownKey)?;
        let provided = hex::decode(&envelope.mac_hex).map_err(|_| DistributedError::InvalidMac)?;
        let mut mac =
            HmacSha256::new_from_slice(secret).map_err(|_| DistributedError::InvalidMac)?;
        let signing_bytes = signing_bytes(
            &envelope.key_id,
            &envelope.nonce,
            envelope.issued_at_secs,
            &envelope.payload,
        )?;
        mac.update(&signing_bytes);
        mac.verify_slice(&provided)
            .map_err(|_| DistributedError::InvalidMac)?;
        self.seen_nonces
            .retain(|_, seen_at| now_secs.saturating_sub(*seen_at) <= self.max_clock_skew_secs * 2);
        self.seen_nonces.insert(envelope.nonce.clone(), now_secs);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeState {
    pub registration: NodeRegistration,
    pub last_seen_secs: u64,
    pub load_percent: u8,
    pub active_jobs: u16,
}

#[derive(Debug)]
pub struct NodeRegistry {
    verifier: EnvelopeVerifier,
    nodes: HashMap<String, NodeState>,
    lease_seconds: u64,
}

impl NodeRegistry {
    pub fn new(verifier: EnvelopeVerifier, lease_seconds: u64) -> Result<Self, DistributedError> {
        if !(5..=3600).contains(&lease_seconds) {
            return Err(DistributedError::InvalidLease);
        }
        Ok(Self {
            verifier,
            nodes: HashMap::new(),
            lease_seconds,
        })
    }

    pub fn verifier_mut(&mut self) -> &mut EnvelopeVerifier {
        &mut self.verifier
    }

    pub fn register(
        &mut self,
        envelope: &AuthenticatedEnvelope<NodeRegistration>,
        now_secs: u64,
    ) -> Result<(), DistributedError> {
        self.verifier.verify(envelope, now_secs)?;
        envelope.payload.validate()?;
        if envelope.payload.key_id != envelope.key_id {
            return Err(DistributedError::KeyIdentityMismatch);
        }
        self.nodes.insert(
            envelope.payload.profile.node_id.clone(),
            NodeState {
                registration: envelope.payload.clone(),
                last_seen_secs: now_secs,
                load_percent: 0,
                active_jobs: 0,
            },
        );
        Ok(())
    }

    pub fn heartbeat(
        &mut self,
        envelope: &AuthenticatedEnvelope<NodeHeartbeat>,
        now_secs: u64,
    ) -> Result<(), DistributedError> {
        self.verifier.verify(envelope, now_secs)?;
        envelope.payload.validate()?;
        let node = self
            .nodes
            .get_mut(&envelope.payload.node_id)
            .ok_or(DistributedError::UnknownNode)?;
        if node.registration.key_id != envelope.key_id {
            return Err(DistributedError::KeyIdentityMismatch);
        }
        if envelope.payload.active_jobs > node.registration.profile.max_parallel_jobs {
            return Err(DistributedError::ActiveJobsExceedCapacity);
        }
        node.last_seen_secs = now_secs;
        node.load_percent = envelope.payload.load_percent;
        node.active_jobs = envelope.payload.active_jobs;
        Ok(())
    }

    pub fn is_online(&self, node_id: &str, now_secs: u64) -> bool {
        self.nodes
            .get(node_id)
            .map(|node| now_secs.saturating_sub(node.last_seen_secs) <= self.lease_seconds)
            .unwrap_or(false)
    }

    pub fn states(&self) -> Vec<&NodeState> {
        let mut states: Vec<_> = self.nodes.values().collect();
        states.sort_by(|a, b| {
            a.registration
                .profile
                .node_id
                .cmp(&b.registration.profile.node_id)
        });
        states
    }

    pub fn allocate_plan(
        &mut self,
        plan: &MultiNodePlan,
        now_secs: u64,
    ) -> Result<MultiNodeAssignment, DistributedError> {
        plan.validate()?;
        let mut used = HashSet::new();
        let mut assignments = Vec::with_capacity(plan.roles.len());

        for role in &plan.roles {
            let mut candidates: Vec<_> = self
                .nodes
                .values()
                .filter(|node| !used.contains(&node.registration.profile.node_id))
                .filter(|node| now_secs.saturating_sub(node.last_seen_secs) <= self.lease_seconds)
                .filter(|node| node.active_jobs < node.registration.profile.max_parallel_jobs)
                .filter(|node| node.load_percent <= role.max_load_percent)
                .filter(|node| role.matches(&node.registration.profile))
                .map(|node| {
                    (
                        node.load_percent,
                        node.active_jobs,
                        node.registration.profile.node_id.clone(),
                    )
                })
                .collect();

            candidates.sort();
            let (_, _, node_id) = candidates
                .into_iter()
                .next()
                .ok_or_else(|| DistributedError::NoEligibleNode(role.role.clone()))?;
            used.insert(node_id.clone());
            assignments.push(RoleAssignment {
                role: role.role.clone(),
                node_id: node_id.clone(),
            });
            if let Some(node) = self.nodes.get_mut(&node_id) {
                node.active_jobs = node.active_jobs.saturating_add(1);
            }
        }

        Ok(MultiNodeAssignment {
            plan_id: plan.plan_id,
            assignments,
        })
    }

    pub fn release_assignment(&mut self, assignment: &MultiNodeAssignment) {
        for role in &assignment.assignments {
            if let Some(node) = self.nodes.get_mut(&role.node_id) {
                node.active_jobs = node.active_jobs.saturating_sub(1);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeRequirement {
    pub role: String,
    pub required_features: BTreeSet<NodeFeature>,
    pub required_labels: BTreeSet<String>,
    pub os: Option<String>,
    pub arch: Option<String>,
    pub max_load_percent: u8,
}

impl NodeRequirement {
    fn validate(&self) -> Result<(), DistributedError> {
        validate_identifier(&self.role, 64)?;
        if self.max_load_percent > 100 {
            return Err(DistributedError::InvalidLoad);
        }
        if let Some(os) = &self.os {
            validate_token(os, 32)?;
        }
        if let Some(arch) = &self.arch {
            validate_token(arch, 32)?;
        }
        for label in &self.required_labels {
            validate_identifier(label, 64)?;
        }
        Ok(())
    }

    fn matches(&self, profile: &NodeProfile) -> bool {
        self.required_features.is_subset(&profile.features)
            && self.required_labels.is_subset(&profile.labels)
            && option_matches(&self.os, &profile.os)
            && option_matches(&self.arch, &profile.arch)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiNodePlan {
    pub plan_id: Uuid,
    pub roles: Vec<NodeRequirement>,
}

impl MultiNodePlan {
    pub fn validate(&self) -> Result<(), DistributedError> {
        if self.roles.is_empty() || self.roles.len() > 32 {
            return Err(DistributedError::InvalidRoleCount);
        }
        let mut names = HashSet::new();
        for role in &self.roles {
            role.validate()?;
            if !names.insert(role.role.clone()) {
                return Err(DistributedError::DuplicateRole(role.role.clone()));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleAssignment {
    pub role: String,
    pub node_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiNodeAssignment {
    pub plan_id: Uuid,
    pub assignments: Vec<RoleAssignment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DistributedArtifact {
    pub name: String,
    pub size_bytes: u64,
    pub sha256: String,
}

impl DistributedArtifact {
    pub fn from_bytes(name: impl Into<String>, bytes: &[u8]) -> Result<Self, DistributedError> {
        let name = name.into();
        validate_artifact_name(&name)?;
        Ok(Self {
            name,
            size_bytes: bytes.len() as u64,
            sha256: hex::encode(Sha256::digest(bytes)),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeResultManifest {
    pub job_id: Uuid,
    pub node_id: String,
    pub status: JobStatus,
    pub summary: String,
    pub artifacts: Vec<DistributedArtifact>,
}

impl NodeResultManifest {
    pub fn validate(&self) -> Result<(), DistributedError> {
        validate_identifier(&self.node_id, 64)?;
        if self.summary.len() > 4096 || self.artifacts.len() > 256 {
            return Err(DistributedError::InvalidResultManifest);
        }
        for artifact in &self.artifacts {
            validate_artifact_name(&artifact.name)?;
            if artifact.sha256.len() != 64
                || !artifact.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(DistributedError::InvalidArtifactDigest);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FaultProfile {
    pub delay_ms: u64,
    pub drop_every_n: Option<u32>,
}

impl FaultProfile {
    pub fn validate(&self) -> Result<(), DistributedError> {
        if self.delay_ms > 30_000 {
            return Err(DistributedError::InvalidFaultProfile);
        }
        if matches!(self.drop_every_n, Some(value) if !(2..=10_000).contains(&value)) {
            return Err(DistributedError::InvalidFaultProfile);
        }
        Ok(())
    }

    #[allow(clippy::manual_is_multiple_of)]
    pub fn should_drop(&self, sequence: u64) -> bool {
        self.drop_every_n
            .map(|n| sequence > 0 && sequence % n as u64 == 0)
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkFixtureReport {
    pub tcp_loopback: bool,
    pub udp_loopback: bool,
    pub dns_localhost: bool,
    pub fault_delay_ms: u64,
    pub fault_drop_sequence: u64,
    pub fault_drop_observed: bool,
}

pub fn run_network_fixtures() -> Result<NetworkFixtureReport, DistributedError> {
    let tcp_loopback = tcp_loopback_round_trip()?;
    let udp_loopback = udp_loopback_round_trip()?;
    let dns_localhost = dns_lookup("localhost")?.iter().any(IpAddr::is_loopback);
    let fault = FaultProfile {
        delay_ms: 5,
        drop_every_n: Some(3),
    };
    fault.validate()?;
    std::thread::sleep(Duration::from_millis(fault.delay_ms));
    let fault_drop_sequence = 3;
    let fault_drop_observed = fault.should_drop(fault_drop_sequence);

    Ok(NetworkFixtureReport {
        tcp_loopback,
        udp_loopback,
        dns_localhost,
        fault_delay_ms: fault.delay_ms,
        fault_drop_sequence,
        fault_drop_observed,
    })
}

pub fn dns_lookup(name: &str) -> Result<Vec<IpAddr>, DistributedError> {
    validate_hostname(name)?;
    let mut addresses: Vec<IpAddr> = (name, 0)
        .to_socket_addrs()?
        .map(|address| address.ip())
        .collect();
    addresses.sort();
    addresses.dedup();
    if addresses.is_empty() {
        return Err(DistributedError::DnsLookupEmpty);
    }
    Ok(addresses)
}

pub fn validate_controller_addr(address: SocketAddr) -> Result<(), DistributedError> {
    if safe_controller_ip(address.ip()) && address.port() != 0 {
        Ok(())
    } else {
        Err(DistributedError::UnsafeControllerAddress)
    }
}

#[derive(Debug)]
pub struct OutboundAgentClient {
    stream: TcpStream,
}

impl OutboundAgentClient {
    pub fn connect(address: SocketAddr, timeout: Duration) -> Result<Self, DistributedError> {
        validate_controller_addr(address)?;
        if timeout.is_zero() || timeout > Duration::from_secs(30) {
            return Err(DistributedError::InvalidConnectTimeout);
        }
        let stream = TcpStream::connect_timeout(&address, timeout)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        Ok(Self { stream })
    }

    pub fn send<T: Serialize>(
        &mut self,
        envelope: &AuthenticatedEnvelope<T>,
    ) -> Result<(), DistributedError> {
        let bytes = serde_json::to_vec(envelope)?;
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(DistributedError::FrameTooLarge);
        }
        self.stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
        self.stream.write_all(&bytes)?;
        self.stream.flush()?;
        Ok(())
    }

    pub fn receive<T: DeserializeOwned>(&mut self) -> Result<T, DistributedError> {
        read_frame(&mut self.stream)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DistributedTask {
    NetworkFixtureSuite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeCommand {
    pub job_id: Uuid,
    pub task: DistributedTask,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrossNodeClientResult {
    pub ack: ControllerAck,
    pub result: NodeResultManifest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrossNodeServerResult {
    pub registration: NodeRegistration,
    pub result: NodeResultManifest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControllerAck {
    pub node_id: String,
    pub accepted: bool,
    pub lease_seconds: u64,
}

pub fn connect_registration_probe(
    controller: SocketAddr,
    registration: NodeRegistration,
    key_id: &str,
    secret: &[u8],
    now_secs: u64,
    timeout: Duration,
) -> Result<CrossNodeClientResult, DistributedError> {
    registration.validate()?;
    if registration.key_id != key_id {
        return Err(DistributedError::KeyIdentityMismatch);
    }

    let node_id = registration.profile.node_id.clone();
    let envelope = AuthenticatedEnvelope::sign(key_id.to_string(), secret, now_secs, registration)?;
    let mut client = OutboundAgentClient::connect(controller, timeout)?;
    client.send(&envelope)?;

    let mut verifier = EnvelopeVerifier::new(30)?;
    verifier.add_key(key_id.to_string(), secret)?;

    let ack_envelope: AuthenticatedEnvelope<ControllerAck> = client.receive()?;
    verifier.verify(&ack_envelope, current_unix_time_secs()?)?;
    if !ack_envelope.payload.accepted || ack_envelope.payload.node_id != node_id {
        return Err(DistributedError::RegistrationRejected);
    }

    let command_envelope: AuthenticatedEnvelope<NodeCommand> = client.receive()?;
    verifier.verify(&command_envelope, current_unix_time_secs()?)?;
    let result = execute_distributed_task(&node_id, &command_envelope.payload)?;

    let result_envelope = AuthenticatedEnvelope::sign(
        key_id.to_string(),
        secret,
        current_unix_time_secs()?,
        result.clone(),
    )?;
    client.send(&result_envelope)?;

    Ok(CrossNodeClientResult {
        ack: ack_envelope.payload,
        result,
    })
}

pub fn serve_registration_probe_once(
    bind: SocketAddr,
    key_id: &str,
    secret: &[u8],
    _now_secs: u64,
    timeout: Duration,
) -> Result<CrossNodeServerResult, DistributedError> {
    validate_controller_addr(bind)?;
    let listener = TcpListener::bind(bind)?;
    serve_registration_probe_listener(listener, key_id, secret, timeout)
}

fn serve_registration_probe_listener(
    listener: TcpListener,
    key_id: &str,
    secret: &[u8],
    timeout: Duration,
) -> Result<CrossNodeServerResult, DistributedError> {
    validate_identifier(key_id, 64)?;
    if secret.len() < 32 {
        return Err(DistributedError::WeakSharedSecret);
    }
    if timeout.is_zero() || timeout > Duration::from_secs(300) {
        return Err(DistributedError::InvalidConnectTimeout);
    }

    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + timeout;
    let mut verifier = EnvelopeVerifier::new(30)?;
    verifier.add_key(key_id.to_string(), secret)?;

    let (mut stream, registration_envelope) = loop {
        if Instant::now() >= deadline {
            return Err(DistributedError::RegistrationProbeTimeout);
        }

        match listener.accept() {
            Ok((mut candidate, _peer)) => {
                candidate.set_nonblocking(false)?;
                let remaining = deadline.saturating_duration_since(Instant::now());
                let per_connection_timeout = remaining.min(Duration::from_secs(5));
                candidate.set_read_timeout(Some(per_connection_timeout))?;
                candidate.set_write_timeout(Some(per_connection_timeout))?;

                let envelope: AuthenticatedEnvelope<NodeRegistration> =
                    match read_frame(&mut candidate) {
                        Ok(envelope) => envelope,
                        Err(error) if is_ignorable_probe_connection_error(&error) => continue,
                        Err(error) => return Err(error),
                    };

                let current_time = current_unix_time_secs()?;
                if verifier.verify(&envelope, current_time).is_err()
                    || envelope.payload.validate().is_err()
                    || envelope.key_id != key_id
                    || envelope.payload.key_id != key_id
                {
                    continue;
                }

                break (candidate, envelope);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(error.into()),
        }
    };

    let current_time = current_unix_time_secs()?;

    let ack = ControllerAck {
        node_id: registration_envelope.payload.profile.node_id.clone(),
        accepted: true,
        lease_seconds: DEFAULT_LEASE_SECONDS,
    };
    let ack_envelope = AuthenticatedEnvelope::sign(key_id.to_string(), secret, current_time, ack)?;
    write_frame(&mut stream, &ack_envelope)?;

    let command = NodeCommand {
        job_id: Uuid::new_v4(),
        task: DistributedTask::NetworkFixtureSuite,
    };
    let command_envelope = AuthenticatedEnvelope::sign(
        key_id.to_string(),
        secret,
        current_unix_time_secs()?,
        command,
    )?;
    write_frame(&mut stream, &command_envelope)?;

    let result_envelope: AuthenticatedEnvelope<NodeResultManifest> = read_frame(&mut stream)?;
    verifier.verify(&result_envelope, current_unix_time_secs()?)?;
    result_envelope.payload.validate()?;
    if result_envelope.payload.node_id != registration_envelope.payload.profile.node_id {
        return Err(DistributedError::ResultNodeMismatch);
    }
    if result_envelope.payload.status != JobStatus::Passed {
        return Err(DistributedError::RemoteTaskFailed);
    }

    Ok(CrossNodeServerResult {
        registration: registration_envelope.payload,
        result: result_envelope.payload,
    })
}

pub fn execute_distributed_task(
    node_id: &str,
    command: &NodeCommand,
) -> Result<NodeResultManifest, DistributedError> {
    validate_identifier(node_id, 64)?;
    match command.task {
        DistributedTask::NetworkFixtureSuite => {
            let report = run_network_fixtures()?;
            let bytes = serde_json::to_vec_pretty(&report)?;
            let artifact = DistributedArtifact::from_bytes("network-report.json", &bytes)?;
            Ok(NodeResultManifest {
                job_id: command.job_id,
                node_id: node_id.to_string(),
                status: JobStatus::Passed,
                summary: "typed Phase 8 network fixture suite passed".into(),
                artifacts: vec![artifact],
            })
        }
    }
}

pub fn write_frame<T: Serialize, W: Write>(
    writer: &mut W,
    value: &T,
) -> Result<(), DistributedError> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(DistributedError::FrameTooLarge);
    }
    writer.write_all(&(bytes.len() as u32).to_be_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()?;
    Ok(())
}

pub fn read_frame<T: DeserializeOwned, R: Read>(reader: &mut R) -> Result<T, DistributedError> {
    let mut length = [0u8; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(DistributedError::FrameTooLarge);
    }
    let mut bytes = vec![0u8; length];
    reader.read_exact(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn loopback_transport_fixture() -> Result<bool, DistributedError> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let address = listener.local_addr()?;
    let server = std::thread::spawn(move || -> Result<(), DistributedError> {
        let (mut stream, _) = listener.accept()?;
        let envelope: AuthenticatedEnvelope<NodeHeartbeat> = read_frame(&mut stream)?;
        let response = serde_json::to_vec(&envelope.payload)?;
        if response.len() > MAX_FRAME_BYTES {
            return Err(DistributedError::FrameTooLarge);
        }
        stream.write_all(&(response.len() as u32).to_be_bytes())?;
        stream.write_all(&response)?;
        Ok(())
    });

    let secret = [0x5au8; 32];
    let heartbeat = NodeHeartbeat {
        node_id: "fixture-node".into(),
        load_percent: 7,
        active_jobs: 0,
    };
    let envelope = AuthenticatedEnvelope::sign("fixture-key", &secret, 1000, heartbeat.clone())?;
    let mut client = OutboundAgentClient::connect(address, Duration::from_secs(5))?;
    client.send(&envelope)?;
    let echoed: NodeHeartbeat = client.receive()?;
    server
        .join()
        .map_err(|_| DistributedError::FixtureThreadPanicked)??;
    Ok(echoed == heartbeat)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DistributedFixtureReport {
    pub registration_authenticated: bool,
    pub replay_rejected: bool,
    pub heartbeat_applied: bool,
    pub distinct_role_assignment: bool,
    pub artifact_hash_verified: bool,
    pub outbound_transport_round_trip: bool,
    pub network: NetworkFixtureReport,
}

pub fn run_distributed_fixtures() -> Result<DistributedFixtureReport, DistributedError> {
    let win_secret = [0x11u8; 32];
    let linux_secret = [0x22u8; 32];
    let mut verifier = EnvelopeVerifier::new(30)?;
    verifier.add_key("win-key", &win_secret)?;
    verifier.add_key("linux-key", &linux_secret)?;
    let mut registry = NodeRegistry::new(verifier, DEFAULT_LEASE_SECONDS)?;

    let win_registration = NodeRegistration {
        protocol_version: PROTOCOL_VERSION,
        profile: NodeProfile {
            node_id: "fixture-win".into(),
            os: "windows".into(),
            arch: "x86_64".into(),
            labels: ["interactive".to_string()].into_iter().collect(),
            features: [
                NodeFeature::Rust,
                NodeFeature::GuiAutomation,
                NodeFeature::TcpFixture,
                NodeFeature::UdpFixture,
            ]
            .into_iter()
            .collect(),
            max_parallel_jobs: 2,
        },
        outbound_only: true,
        key_id: "win-key".into(),
    };
    let linux_registration = NodeRegistration {
        protocol_version: PROTOCOL_VERSION,
        profile: NodeProfile {
            node_id: "fixture-linux".into(),
            os: "linux".into(),
            arch: "x86_64".into(),
            labels: ["container".to_string()].into_iter().collect(),
            features: [
                NodeFeature::Rust,
                NodeFeature::Docker,
                NodeFeature::TcpFixture,
                NodeFeature::DnsFixture,
                NodeFeature::FaultInjection,
            ]
            .into_iter()
            .collect(),
            max_parallel_jobs: 4,
        },
        outbound_only: true,
        key_id: "linux-key".into(),
    };

    let win_envelope =
        AuthenticatedEnvelope::sign("win-key", &win_secret, 1_000, win_registration)?;
    registry.register(&win_envelope, 1_000)?;
    let replay_rejected = matches!(
        registry.register(&win_envelope, 1_000),
        Err(DistributedError::ReplayDetected)
    );

    let linux_envelope =
        AuthenticatedEnvelope::sign("linux-key", &linux_secret, 1_000, linux_registration)?;
    registry.register(&linux_envelope, 1_000)?;

    let heartbeat = AuthenticatedEnvelope::sign(
        "win-key",
        &win_secret,
        1_005,
        NodeHeartbeat {
            node_id: "fixture-win".into(),
            load_percent: 12,
            active_jobs: 0,
        },
    )?;
    registry.heartbeat(&heartbeat, 1_005)?;
    let heartbeat_applied = registry
        .states()
        .into_iter()
        .find(|state| state.registration.profile.node_id == "fixture-win")
        .map(|state| state.load_percent == 12 && state.last_seen_secs == 1_005)
        .unwrap_or(false);

    let plan = MultiNodePlan {
        plan_id: Uuid::new_v4(),
        roles: vec![
            NodeRequirement {
                role: "interactive-gui".into(),
                required_features: [NodeFeature::GuiAutomation].into_iter().collect(),
                required_labels: ["interactive".to_string()].into_iter().collect(),
                os: Some("windows".into()),
                arch: Some("x86_64".into()),
                max_load_percent: 80,
            },
            NodeRequirement {
                role: "network-fault".into(),
                required_features: [NodeFeature::DnsFixture, NodeFeature::FaultInjection]
                    .into_iter()
                    .collect(),
                required_labels: ["container".to_string()].into_iter().collect(),
                os: Some("linux".into()),
                arch: Some("x86_64".into()),
                max_load_percent: 80,
            },
        ],
    };
    let assignment = registry.allocate_plan(&plan, 1_005)?;
    let distinct_role_assignment = assignment.assignments.len() == 2
        && assignment.assignments[0].node_id != assignment.assignments[1].node_id;
    registry.release_assignment(&assignment);

    let artifact = DistributedArtifact::from_bytes("result.json", br#"{"status":"ok"}"#)?;
    let artifact_hash_verified =
        artifact.sha256 == "a29ee2b15c494311c52521766e44af56a3ad2248e7a8ab465e5206463c13d288";

    let outbound_transport_round_trip = loopback_transport_fixture()?;
    let network = run_network_fixtures()?;

    Ok(DistributedFixtureReport {
        registration_authenticated: true,
        replay_rejected,
        heartbeat_applied,
        distinct_role_assignment,
        artifact_hash_verified,
        outbound_transport_round_trip,
        network,
    })
}

fn tcp_loopback_round_trip() -> Result<bool, DistributedError> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let address = listener.local_addr()?;
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        let (mut stream, _) = listener.accept()?;
        let mut request = [0u8; 11];
        stream.read_exact(&mut request)?;
        if &request != b"dragonforge" {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "unexpected payload",
            ));
        }
        stream.write_all(b"ok")?;
        Ok(())
    });

    let mut client = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
    client.write_all(b"dragonforge")?;
    let mut response = [0u8; 2];
    client.read_exact(&mut response)?;
    server
        .join()
        .map_err(|_| DistributedError::FixtureThreadPanicked)??;
    Ok(&response == b"ok")
}

fn udp_loopback_round_trip() -> Result<bool, DistributedError> {
    let server = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
    server.set_read_timeout(Some(Duration::from_secs(5)))?;
    let address = server.local_addr()?;
    let thread = std::thread::spawn(move || -> std::io::Result<()> {
        let mut request = [0u8; 64];
        let (size, peer) = server.recv_from(&mut request)?;
        if &request[..size] != b"dragonforge" {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "unexpected payload",
            ));
        }
        server.send_to(b"ok", peer)?;
        Ok(())
    });

    let client = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
    client.set_read_timeout(Some(Duration::from_secs(5)))?;
    client.send_to(b"dragonforge", address)?;
    let mut response = [0u8; 2];
    let (size, _) = client.recv_from(&mut response)?;
    thread
        .join()
        .map_err(|_| DistributedError::FixtureThreadPanicked)??;
    Ok(size == 2 && &response == b"ok")
}

fn safe_controller_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        IpAddr::V6(ip) => ip.is_loopback() || is_ipv6_unique_local(ip) || is_ipv6_link_local(ip),
    }
}

fn is_ipv6_unique_local(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xfe00) == 0xfc00
}

fn is_ipv6_link_local(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

fn is_ignorable_probe_connection_error(error: &DistributedError) -> bool {
    match error {
        DistributedError::Io(error) => matches!(
            error.kind(),
            std::io::ErrorKind::UnexpectedEof
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::TimedOut
                | std::io::ErrorKind::WouldBlock
        ),
        DistributedError::FrameTooLarge | DistributedError::Json(_) => true,
        _ => false,
    }
}

fn current_unix_time_secs() -> Result<u64, DistributedError> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| DistributedError::SystemClockBeforeUnixEpoch)?
        .as_secs())
}

fn compute_mac<T: Serialize>(
    key_id: &str,
    nonce: &str,
    issued_at_secs: u64,
    payload: &T,
    secret: &[u8],
) -> Result<String, DistributedError> {
    let bytes = signing_bytes(key_id, nonce, issued_at_secs, payload)?;
    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| DistributedError::InvalidMac)?;
    mac.update(&bytes);
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn signing_bytes<T: Serialize>(
    key_id: &str,
    nonce: &str,
    issued_at_secs: u64,
    payload: &T,
) -> Result<Vec<u8>, DistributedError> {
    Ok(serde_json::to_vec(&(
        key_id,
        nonce,
        issued_at_secs,
        payload,
    ))?)
}

fn option_matches(expected: &Option<String>, actual: &str) -> bool {
    match expected {
        Some(value) => value == actual,
        None => true,
    }
}

fn validate_identifier(value: &str, max_len: usize) -> Result<(), DistributedError> {
    let valid = !value.is_empty()
        && value.len() <= max_len
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if valid {
        Ok(())
    } else {
        Err(DistributedError::InvalidIdentifier)
    }
}

fn validate_token(value: &str, max_len: usize) -> Result<(), DistributedError> {
    validate_identifier(value, max_len)
}

fn validate_artifact_name(value: &str) -> Result<(), DistributedError> {
    validate_identifier(value, 128)?;
    if value.contains("..") {
        return Err(DistributedError::InvalidArtifactName);
    }
    Ok(())
}

fn validate_hostname(value: &str) -> Result<(), DistributedError> {
    if value.is_empty()
        || value.len() > 253
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.'))
        || value.starts_with('-')
        || value.ends_with('-')
        || value.contains("..")
    {
        return Err(DistributedError::InvalidHostname);
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum DistributedError {
    #[error("protocol version mismatch")]
    ProtocolMismatch,
    #[error("distributed agents must use outbound-only connections")]
    InboundAgentListenerForbidden,
    #[error("invalid identifier")]
    InvalidIdentifier,
    #[error("invalid node parallelism")]
    InvalidParallelism,
    #[error("too many node labels")]
    TooManyLabels,
    #[error("shared secret must be at least 32 bytes")]
    WeakSharedSecret,
    #[error("invalid authentication clock skew")]
    InvalidClockSkew,
    #[error("message authentication failed")]
    InvalidMac,
    #[error("unknown authentication key")]
    UnknownKey,
    #[error("authenticated message is outside the accepted clock window")]
    StaleEnvelope,
    #[error("authenticated message replay detected")]
    ReplayDetected,
    #[error("node key identity mismatch")]
    KeyIdentityMismatch,
    #[error("unknown node")]
    UnknownNode,
    #[error("node active jobs exceed capacity")]
    ActiveJobsExceedCapacity,
    #[error("invalid node load")]
    InvalidLoad,
    #[error("invalid node lease")]
    InvalidLease,
    #[error("invalid role count")]
    InvalidRoleCount,
    #[error("duplicate role: {0}")]
    DuplicateRole(String),
    #[error("no eligible node for role: {0}")]
    NoEligibleNode(String),
    #[error("invalid result manifest")]
    InvalidResultManifest,
    #[error("invalid artifact name")]
    InvalidArtifactName,
    #[error("invalid artifact digest")]
    InvalidArtifactDigest,
    #[error("invalid fault profile")]
    InvalidFaultProfile,
    #[error("invalid hostname")]
    InvalidHostname,
    #[error("DNS lookup returned no addresses")]
    DnsLookupEmpty,
    #[error("controller address must be loopback/private/link-local with a nonzero port")]
    UnsafeControllerAddress,
    #[error("invalid controller connect timeout")]
    InvalidConnectTimeout,
    #[error("transport frame is empty or too large")]
    FrameTooLarge,
    #[error("fixture thread panicked")]
    FixtureThreadPanicked,
    #[error("controller rejected node registration")]
    RegistrationRejected,
    #[error("registration probe timed out waiting for an outbound node connection")]
    RegistrationProbeTimeout,
    #[error("system clock is before the Unix epoch")]
    SystemClockBeforeUnixEpoch,
    #[error("remote result node does not match the authenticated registration")]
    ResultNodeMismatch,
    #[error("remote typed task failed")]
    RemoteTaskFailed,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str, os: &str, load_features: &[NodeFeature]) -> NodeProfile {
        NodeProfile {
            node_id: id.into(),
            os: os.into(),
            arch: "x86_64".into(),
            labels: BTreeSet::new(),
            features: load_features.iter().copied().collect(),
            max_parallel_jobs: 2,
        }
    }

    #[test]
    fn registration_requires_outbound_only() {
        let registration = NodeRegistration {
            protocol_version: PROTOCOL_VERSION,
            profile: profile("node-1", "windows", &[NodeFeature::Rust]),
            outbound_only: false,
            key_id: "node-1-key".into(),
        };
        assert!(matches!(
            registration.validate(),
            Err(DistributedError::InboundAgentListenerForbidden)
        ));
    }

    #[test]
    fn signed_envelope_detects_replay() {
        let secret = [7u8; 32];
        let mut verifier = EnvelopeVerifier::new(30).unwrap();
        verifier.add_key("node-key", &secret).unwrap();
        let heartbeat = NodeHeartbeat {
            node_id: "node-1".into(),
            load_percent: 10,
            active_jobs: 0,
        };
        let envelope = AuthenticatedEnvelope::sign("node-key", &secret, 100, heartbeat).unwrap();
        verifier.verify(&envelope, 100).unwrap();
        assert!(matches!(
            verifier.verify(&envelope, 100),
            Err(DistributedError::ReplayDetected)
        ));
    }

    #[test]
    fn scheduler_uses_distinct_capable_nodes() {
        let secret1 = [1u8; 32];
        let secret2 = [2u8; 32];
        let mut verifier = EnvelopeVerifier::new(30).unwrap();
        verifier.add_key("win-key", &secret1).unwrap();
        verifier.add_key("linux-key", &secret2).unwrap();
        let mut registry = NodeRegistry::new(verifier, DEFAULT_LEASE_SECONDS).unwrap();

        let win = NodeRegistration {
            protocol_version: PROTOCOL_VERSION,
            profile: profile(
                "win-1",
                "windows",
                &[
                    NodeFeature::Rust,
                    NodeFeature::GuiAutomation,
                    NodeFeature::TcpFixture,
                ],
            ),
            outbound_only: true,
            key_id: "win-key".into(),
        };
        let linux = NodeRegistration {
            protocol_version: PROTOCOL_VERSION,
            profile: profile(
                "linux-1",
                "linux",
                &[
                    NodeFeature::Rust,
                    NodeFeature::TcpFixture,
                    NodeFeature::DnsFixture,
                ],
            ),
            outbound_only: true,
            key_id: "linux-key".into(),
        };
        registry
            .register(
                &AuthenticatedEnvelope::sign("win-key", &secret1, 100, win).unwrap(),
                100,
            )
            .unwrap();
        registry
            .register(
                &AuthenticatedEnvelope::sign("linux-key", &secret2, 100, linux).unwrap(),
                100,
            )
            .unwrap();

        let plan = MultiNodePlan {
            plan_id: Uuid::new_v4(),
            roles: vec![
                NodeRequirement {
                    role: "gui".into(),
                    required_features: [NodeFeature::GuiAutomation].into_iter().collect(),
                    required_labels: BTreeSet::new(),
                    os: Some("windows".into()),
                    arch: None,
                    max_load_percent: 80,
                },
                NodeRequirement {
                    role: "dns".into(),
                    required_features: [NodeFeature::DnsFixture].into_iter().collect(),
                    required_labels: BTreeSet::new(),
                    os: Some("linux".into()),
                    arch: None,
                    max_load_percent: 80,
                },
            ],
        };

        let assignment = registry.allocate_plan(&plan, 100).unwrap();
        assert_eq!(assignment.assignments.len(), 2);
        assert_ne!(
            assignment.assignments[0].node_id,
            assignment.assignments[1].node_id
        );
    }

    #[test]
    fn controller_ignores_health_probe_before_real_node() {
        let secret = [0x33u8; 32];
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();

        let server_secret = secret;
        let server = std::thread::spawn(move || {
            serve_registration_probe_listener(
                listener,
                "probe-key",
                &server_secret,
                Duration::from_secs(10),
            )
            .unwrap()
        });

        std::thread::sleep(Duration::from_millis(25));
        let probe = TcpStream::connect(address).unwrap();
        drop(probe);

        let registration = NodeRegistration {
            protocol_version: PROTOCOL_VERSION,
            profile: profile(
                "node-after-probe",
                "windows",
                &[
                    NodeFeature::TcpFixture,
                    NodeFeature::UdpFixture,
                    NodeFeature::DnsFixture,
                ],
            ),
            outbound_only: true,
            key_id: "probe-key".into(),
        };

        let result = connect_registration_probe(
            address,
            registration,
            "probe-key",
            &secret,
            current_unix_time_secs().unwrap(),
            Duration::from_secs(5),
        )
        .unwrap();

        assert_eq!(result.ack.node_id, "node-after-probe");
        assert_eq!(result.result.status, JobStatus::Passed);

        let server_result = server.join().unwrap();
        assert_eq!(
            server_result.registration.profile.node_id,
            "node-after-probe"
        );
        assert_eq!(server_result.result.status, JobStatus::Passed);
    }

    #[test]
    fn public_controller_addresses_are_rejected() {
        let public: SocketAddr = "8.8.8.8:443".parse().unwrap();
        assert!(validate_controller_addr(public).is_err());
        let private: SocketAddr = "127.0.0.1:443".parse().unwrap();
        assert!(validate_controller_addr(private).is_ok());
    }

    #[test]
    fn artifacts_are_hashed() {
        let artifact = DistributedArtifact::from_bytes("result.json", b"dragonforge").unwrap();
        assert_eq!(artifact.size_bytes, 11);
        assert_eq!(artifact.sha256.len(), 64);
    }

    #[test]
    fn deterministic_fault_profile_drops_expected_sequence() {
        let profile = FaultProfile {
            delay_ms: 10,
            drop_every_n: Some(4),
        };
        assert!(profile.should_drop(4));
        assert!(!profile.should_drop(5));
    }
}
