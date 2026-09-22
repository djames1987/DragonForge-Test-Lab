use rustls::{
    pki_types::{CertificateDer, PrivateKeyDer, ServerName},
    ClientConfig, RootCertStore, ServerConfig,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::Arc,
    time::Duration,
};
use thiserror::Error;

pub const MAX_CERTIFICATE_CHAIN: usize = 8;
pub const MAX_CERTIFICATE_BYTES: usize = 256 * 1024;
pub const MAX_TRUSTED_IDENTITIES: usize = 4096;
pub const MAX_CERTIFICATES_PER_IDENTITY: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CertificateFingerprint(String);

impl CertificateFingerprint {
    pub fn from_der(der: &[u8]) -> Result<Self, IdentityError> {
        if der.is_empty() || der.len() > MAX_CERTIFICATE_BYTES {
            return Err(IdentityError::InvalidCertificate);
        }
        Ok(Self(hex::encode(Sha256::digest(der))))
    }

    pub fn as_hex(&self) -> &str {
        &self.0
    }

    fn validate(&self) -> Result<(), IdentityError> {
        if self.0.len() != 64 || !self.0.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(IdentityError::InvalidCertificateFingerprint);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedCertificate {
    pub fingerprint: CertificateFingerprint,
    pub generation: u32,
    pub valid_from_secs: u64,
    pub valid_until_secs: u64,
}

impl TrustedCertificate {
    fn validate(&self) -> Result<(), IdentityError> {
        if self.generation == 0 || self.valid_until_secs <= self.valid_from_secs {
            return Err(IdentityError::InvalidValidityWindow);
        }
        Ok(())
    }

    fn valid_at(&self, now_secs: u64) -> bool {
        self.valid_from_secs <= now_secs && now_secs <= self.valid_until_secs
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedIdentity {
    pub node_id: String,
    pub certificates: Vec<TrustedCertificate>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IdentityTrustStore {
    identities: BTreeMap<String, TrustedIdentity>,
    revoked_fingerprints: BTreeSet<CertificateFingerprint>,
}

impl IdentityTrustStore {
    pub fn enroll(
        &mut self,
        node_id: impl Into<String>,
        certificate_der: &[u8],
        valid_from_secs: u64,
        valid_until_secs: u64,
    ) -> Result<TrustedCertificate, IdentityError> {
        let node_id = node_id.into();
        validate_identifier(&node_id)?;
        if self.identities.len() >= MAX_TRUSTED_IDENTITIES && !self.identities.contains_key(&node_id)
        {
            return Err(IdentityError::TooManyIdentities);
        }

        let fingerprint = CertificateFingerprint::from_der(certificate_der)?;
        if self.revoked_fingerprints.contains(&fingerprint) {
            return Err(IdentityError::CertificateRevoked);
        }

        let identity = self
            .identities
            .entry(node_id.clone())
            .or_insert_with(|| TrustedIdentity {
                node_id,
                certificates: Vec::new(),
            });

        if identity.certificates.len() >= MAX_CERTIFICATES_PER_IDENTITY {
            return Err(IdentityError::TooManyCertificates);
        }
        if identity
            .certificates
            .iter()
            .any(|certificate| certificate.fingerprint == fingerprint)
        {
            return Err(IdentityError::CertificateAlreadyEnrolled);
        }

        let generation = identity
            .certificates
            .iter()
            .map(|certificate| certificate.generation)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let certificate = TrustedCertificate {
            fingerprint,
            generation,
            valid_from_secs,
            valid_until_secs,
        };
        certificate.validate()?;
        identity.certificates.push(certificate.clone());
        identity.certificates.sort_by_key(|item| item.generation);
        Ok(certificate)
    }

    pub fn renew(
        &mut self,
        node_id: &str,
        certificate_der: &[u8],
        valid_from_secs: u64,
        valid_until_secs: u64,
        previous_generation_valid_until_secs: u64,
    ) -> Result<TrustedCertificate, IdentityError> {
        validate_identifier(node_id)?;
        let current_generation = self
            .identities
            .get(node_id)
            .and_then(|identity| identity.certificates.last())
            .map(|certificate| certificate.generation)
            .ok_or(IdentityError::UnknownIdentity)?;

        {
            let identity = self
                .identities
                .get_mut(node_id)
                .ok_or(IdentityError::UnknownIdentity)?;
            let current = identity
                .certificates
                .iter_mut()
                .find(|certificate| certificate.generation == current_generation)
                .ok_or(IdentityError::UnknownCertificate)?;
            if previous_generation_valid_until_secs < valid_from_secs
                || previous_generation_valid_until_secs > current.valid_until_secs
            {
                return Err(IdentityError::InvalidRotationOverlap);
            }
            current.valid_until_secs = previous_generation_valid_until_secs;
        }

        self.enroll(
            node_id.to_owned(),
            certificate_der,
            valid_from_secs,
            valid_until_secs,
        )
    }

    pub fn revoke_certificate(
        &mut self,
        fingerprint: &CertificateFingerprint,
    ) -> Result<(), IdentityError> {
        let exists = self.identities.values().any(|identity| {
            identity
                .certificates
                .iter()
                .any(|certificate| &certificate.fingerprint == fingerprint)
        });
        if !exists {
            return Err(IdentityError::UnknownCertificate);
        }
        self.revoked_fingerprints.insert(fingerprint.clone());
        Ok(())
    }

    pub fn revoke_identity(&mut self, node_id: &str) -> Result<usize, IdentityError> {
        let identity = self
            .identities
            .get(node_id)
            .ok_or(IdentityError::UnknownIdentity)?;
        let mut count = 0usize;
        for certificate in &identity.certificates {
            if self
                .revoked_fingerprints
                .insert(certificate.fingerprint.clone())
            {
                count += 1;
            }
        }
        Ok(count)
    }

    pub fn verify_peer(
        &self,
        node_id: &str,
        certificate_der: &[u8],
        now_secs: u64,
    ) -> Result<&TrustedCertificate, IdentityError> {
        validate_identifier(node_id)?;
        let fingerprint = CertificateFingerprint::from_der(certificate_der)?;
        if self.revoked_fingerprints.contains(&fingerprint) {
            return Err(IdentityError::CertificateRevoked);
        }

        let identity = self
            .identities
            .get(node_id)
            .ok_or(IdentityError::UnknownIdentity)?;
        let certificate = identity
            .certificates
            .iter()
            .find(|certificate| certificate.fingerprint == fingerprint)
            .ok_or(IdentityError::IdentityCertificateMismatch)?;
        if !certificate.valid_at(now_secs) {
            return Err(IdentityError::CertificateOutsideValidity);
        }
        Ok(certificate)
    }

    pub fn identity(&self, node_id: &str) -> Option<&TrustedIdentity> {
        self.identities.get(node_id)
    }

    pub fn is_revoked(&self, fingerprint: &CertificateFingerprint) -> bool {
        self.revoked_fingerprints.contains(fingerprint)
    }

    pub fn to_json(&self) -> Result<String, IdentityError> {
        serde_json::to_string_pretty(self).map_err(IdentityError::Json)
    }

    pub fn from_json(json: &str) -> Result<Self, IdentityError> {
        let store: Self = serde_json::from_str(json).map_err(IdentityError::Json)?;
        if store.identities.len() > MAX_TRUSTED_IDENTITIES {
            return Err(IdentityError::TooManyIdentities);
        }
        for (node_id, identity) in &store.identities {
            validate_identifier(node_id)?;
            if identity.node_id != *node_id {
                return Err(IdentityError::InvalidIdentity);
            }
            if identity.certificates.len() > MAX_CERTIFICATES_PER_IDENTITY {
                return Err(IdentityError::TooManyCertificates);
            }
            let mut generations = BTreeSet::new();
            let mut fingerprints = BTreeSet::new();
            for certificate in &identity.certificates {
                certificate.validate()?;
                certificate.fingerprint.validate()?;
                if !generations.insert(certificate.generation)
                    || !fingerprints.insert(certificate.fingerprint.clone())
                {
                    return Err(IdentityError::InvalidTrustState);
                }
            }
        }
        for fingerprint in &store.revoked_fingerprints {
            fingerprint.validate()?;
        }
        Ok(store)
    }
}

pub struct CertificateMaterial {
    pub certificate_chain: Vec<CertificateDer<'static>>,
    pub private_key: PrivateKeyDer<'static>,
}

impl CertificateMaterial {
    pub fn from_pem(certificate_pem: &[u8], private_key_pem: &[u8]) -> Result<Self, IdentityError> {
        let mut certificate_reader = Cursor::new(certificate_pem);
        let certificate_chain = rustls_pemfile::certs(&mut certificate_reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(IdentityError::Io)?;
        if certificate_chain.is_empty() || certificate_chain.len() > MAX_CERTIFICATE_CHAIN {
            return Err(IdentityError::InvalidCertificateChain);
        }
        let total_certificate_bytes = certificate_chain
            .iter()
            .map(|certificate| certificate.as_ref().len())
            .sum::<usize>();
        if total_certificate_bytes > MAX_CERTIFICATE_BYTES {
            return Err(IdentityError::InvalidCertificateChain);
        }

        let mut key_reader = Cursor::new(private_key_pem);
        let private_key =
            rustls_pemfile::private_key(&mut key_reader).map_err(IdentityError::Io)?;
        let private_key = private_key.ok_or(IdentityError::MissingPrivateKey)?;

        Ok(Self {
            certificate_chain,
            private_key,
        })
    }
}

pub fn root_store_from_pem(ca_pem: &[u8]) -> Result<RootCertStore, IdentityError> {
    let mut reader = Cursor::new(ca_pem);
    let certificates = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(IdentityError::Io)?;
    if certificates.is_empty() || certificates.len() > MAX_CERTIFICATE_CHAIN {
        return Err(IdentityError::InvalidCertificateChain);
    }

    let mut roots = RootCertStore::empty();
    for certificate in certificates {
        roots
            .add(certificate)
            .map_err(|error| IdentityError::Tls(error.to_string()))?;
    }
    Ok(roots)
}

pub fn build_client_config(
    ca_pem: &[u8],
    client: CertificateMaterial,
) -> Result<ClientConfig, IdentityError> {
    let roots = root_store_from_pem(ca_pem)?;
    ClientConfig::builder()
        .with_root_certificates(roots)
        .with_client_auth_cert(client.certificate_chain, client.private_key)
        .map_err(|error| IdentityError::Tls(error.to_string()))
}

pub fn build_server_config(
    ca_pem: &[u8],
    server: CertificateMaterial,
) -> Result<ServerConfig, IdentityError> {
    let roots = root_store_from_pem(ca_pem)?;
    let verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(roots))
        .build()
        .map_err(|error| IdentityError::Tls(error.to_string()))?;
    ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(server.certificate_chain, server.private_key)
        .map_err(|error| IdentityError::Tls(error.to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MtlsFixtureReport {
    pub encrypted_round_trip: bool,
    pub client_certificate_observed: bool,
    pub server_certificate_observed: bool,
    pub node_identity_verified: bool,
    pub renewal_verified: bool,
    pub revocation_verified: bool,
}

pub fn run_mtls_fixture() -> Result<MtlsFixtureReport, IdentityError> {
    let fixture = FixtureCertificates::generate()?;

    let client_material =
        CertificateMaterial::from_pem(fixture.client_cert_pem.as_bytes(), fixture.client_key_pem.as_bytes())?;
    let server_material =
        CertificateMaterial::from_pem(fixture.server_cert_pem.as_bytes(), fixture.server_key_pem.as_bytes())?;
    let client_config = Arc::new(build_client_config(fixture.ca_cert_pem.as_bytes(), client_material)?);
    let server_config = Arc::new(build_server_config(fixture.ca_cert_pem.as_bytes(), server_material)?);

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server_thread = std::thread::spawn(move || -> Result<bool, String> {
        let (tcp, _) = listener.accept().map_err(|error| error.to_string())?;
        tcp.set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|error| error.to_string())?;
        tcp.set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(|error| error.to_string())?;
        let connection =
            rustls::ServerConnection::new(server_config).map_err(|error| error.to_string())?;
        let mut stream = rustls::StreamOwned::new(connection, tcp);
        let mut request = [0u8; 10];
        stream
            .read_exact(&mut request)
            .map_err(|error| error.to_string())?;
        if &request != b"dragonforge" {
            return Err("unexpected mTLS fixture payload".to_owned());
        }
        let client_certificate_observed = stream
            .conn
            .peer_certificates()
            .map(|certificates| !certificates.is_empty())
            .unwrap_or(false);
        stream
            .write_all(b"authenticated")
            .map_err(|error| error.to_string())?;
        stream.flush().map_err(|error| error.to_string())?;
        Ok(client_certificate_observed)
    });

    let tcp = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
    tcp.set_read_timeout(Some(Duration::from_secs(5)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(5)))?;
    let server_name =
        ServerName::try_from("localhost").map_err(|_| IdentityError::InvalidServerName)?;
    let connection = rustls::ClientConnection::new(client_config, server_name)
        .map_err(|error| IdentityError::Tls(error.to_string()))?;
    let mut stream = rustls::StreamOwned::new(connection, tcp);
    stream.write_all(b"dragonforge")?;
    stream.flush()?;
    let mut response = [0u8; 13];
    stream.read_exact(&mut response)?;
    let server_certificate_observed = stream
        .conn
        .peer_certificates()
        .map(|certificates| !certificates.is_empty())
        .unwrap_or(false);
    let encrypted_round_trip = &response == b"authenticated";
    let client_certificate_observed = server_thread
        .join()
        .map_err(|_| IdentityError::FixtureThreadPanicked)?
        .map_err(IdentityError::Fixture)?;

    let client_der = fixture.client_cert_der.as_ref();
    let rotated_der = fixture.rotated_client_cert_der.as_ref();
    let mut trust = IdentityTrustStore::default();
    let first = trust.enroll("fixture-node", client_der, 100, 1_000)?;
    let node_identity_verified = trust
        .verify_peer("fixture-node", client_der, 200)
        .map(|certificate| certificate.generation == first.generation)
        .unwrap_or(false);

    let renewed = trust.renew("fixture-node", rotated_der, 500, 2_000, 700)?;
    let renewal_verified = renewed.generation == first.generation + 1
        && trust.verify_peer("fixture-node", client_der, 600).is_ok()
        && trust.verify_peer("fixture-node", client_der, 701).is_err()
        && trust.verify_peer("fixture-node", rotated_der, 701).is_ok();

    trust.revoke_certificate(&renewed.fingerprint)?;
    let revocation_verified = trust
        .verify_peer("fixture-node", rotated_der, 800)
        .is_err();

    Ok(MtlsFixtureReport {
        encrypted_round_trip,
        client_certificate_observed,
        server_certificate_observed,
        node_identity_verified,
        renewal_verified,
        revocation_verified,
    })
}

struct FixtureCertificates {
    ca_cert_pem: String,
    server_cert_pem: String,
    server_key_pem: String,
    client_cert_pem: String,
    client_key_pem: String,
    client_cert_der: CertificateDer<'static>,
    rotated_client_cert_der: CertificateDer<'static>,
}

impl FixtureCertificates {
    fn generate() -> Result<Self, IdentityError> {
        use rcgen::{BasicConstraints, CertificateParams, CertifiedIssuer, IsCa, KeyPair};

        let mut ca_params = CertificateParams::new(Vec::<String>::new())
            .map_err(|error| IdentityError::CertificateGeneration(error.to_string()))?;
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let ca_key = KeyPair::generate()
            .map_err(|error| IdentityError::CertificateGeneration(error.to_string()))?;
        let ca = CertifiedIssuer::self_signed(ca_params, ca_key)
            .map_err(|error| IdentityError::CertificateGeneration(error.to_string()))?;

        let server_key = KeyPair::generate()
            .map_err(|error| IdentityError::CertificateGeneration(error.to_string()))?;
        let server_params = CertificateParams::new(vec!["localhost".to_owned()])
            .map_err(|error| IdentityError::CertificateGeneration(error.to_string()))?;
        let server_cert = server_params
            .signed_by(&server_key, &ca)
            .map_err(|error| IdentityError::CertificateGeneration(error.to_string()))?;

        let client_key = KeyPair::generate()
            .map_err(|error| IdentityError::CertificateGeneration(error.to_string()))?;
        let client_params = CertificateParams::new(Vec::<String>::new())
            .map_err(|error| IdentityError::CertificateGeneration(error.to_string()))?;
        let client_cert = client_params
            .signed_by(&client_key, &ca)
            .map_err(|error| IdentityError::CertificateGeneration(error.to_string()))?;

        let rotated_client_key = KeyPair::generate()
            .map_err(|error| IdentityError::CertificateGeneration(error.to_string()))?;
        let rotated_client_params = CertificateParams::new(Vec::<String>::new())
            .map_err(|error| IdentityError::CertificateGeneration(error.to_string()))?;
        let rotated_client_cert = rotated_client_params
            .signed_by(&rotated_client_key, &ca)
            .map_err(|error| IdentityError::CertificateGeneration(error.to_string()))?;

        Ok(Self {
            ca_cert_pem: ca.pem(),
            server_cert_pem: server_cert.pem(),
            server_key_pem: server_key.serialize_pem(),
            client_cert_pem: client_cert.pem(),
            client_key_pem: client_key.serialize_pem(),
            client_cert_der: client_cert.der().clone(),
            rotated_client_cert_der: rotated_client_cert.der().clone(),
        })
    }
}

pub fn validate_private_controller_address(address: SocketAddr) -> Result<(), IdentityError> {
    let safe = match address.ip() {
        std::net::IpAddr::V4(ip) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        std::net::IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unicast_link_local()
                || (ip.segments()[0] & 0xfe00) == 0xfc00
        }
    };
    if safe && address.port() != 0 {
        Ok(())
    } else {
        Err(IdentityError::UnsafeControllerAddress)
    }
}

fn validate_identifier(value: &str) -> Result<(), IdentityError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(IdentityError::InvalidIdentity);
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("invalid certificate")]
    InvalidCertificate,
    #[error("invalid certificate chain")]
    InvalidCertificateChain,
    #[error("invalid certificate fingerprint")]
    InvalidCertificateFingerprint,
    #[error("invalid serialized trust state")]
    InvalidTrustState,
    #[error("private key is missing")]
    MissingPrivateKey,
    #[error("invalid identity")]
    InvalidIdentity,
    #[error("identity trust store is full")]
    TooManyIdentities,
    #[error("identity has too many certificates")]
    TooManyCertificates,
    #[error("certificate is already enrolled")]
    CertificateAlreadyEnrolled,
    #[error("identity is not enrolled")]
    UnknownIdentity,
    #[error("certificate is not enrolled")]
    UnknownCertificate,
    #[error("certificate does not belong to requested identity")]
    IdentityCertificateMismatch,
    #[error("certificate has been revoked")]
    CertificateRevoked,
    #[error("certificate is outside its accepted validity window")]
    CertificateOutsideValidity,
    #[error("invalid certificate validity window")]
    InvalidValidityWindow,
    #[error("invalid certificate rotation overlap")]
    InvalidRotationOverlap,
    #[error("invalid TLS server name")]
    InvalidServerName,
    #[error("controller address is not loopback/private/link-local")]
    UnsafeControllerAddress,
    #[error("TLS configuration/handshake error: {0}")]
    Tls(String),
    #[error("certificate generation error: {0}")]
    CertificateGeneration(String),
    #[error("fixture error: {0}")]
    Fixture(String),
    #[error("fixture thread panicked")]
    FixtureThreadPanicked,
    #[error("JSON serialization error: {0}")]
    Json(serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_store_binds_certificate_to_node_identity() {
        let cert_a = b"certificate-a";
        let cert_b = b"certificate-b";
        let mut store = IdentityTrustStore::default();
        store.enroll("node-a", cert_a, 10, 100).unwrap();
        store.enroll("node-b", cert_b, 10, 100).unwrap();

        assert!(store.verify_peer("node-a", cert_a, 50).is_ok());
        assert!(matches!(
            store.verify_peer("node-a", cert_b, 50),
            Err(IdentityError::IdentityCertificateMismatch)
        ));
    }

    #[test]
    fn renewal_supports_bounded_overlap_then_retires_old_generation() {
        let mut store = IdentityTrustStore::default();
        let first = store.enroll("node-a", b"cert-1", 100, 1_000).unwrap();
        let second = store
            .renew("node-a", b"cert-2", 500, 2_000, 700)
            .unwrap();

        assert_eq!(second.generation, first.generation + 1);
        assert!(store.verify_peer("node-a", b"cert-1", 650).is_ok());
        assert!(store.verify_peer("node-a", b"cert-1", 701).is_err());
        assert!(store.verify_peer("node-a", b"cert-2", 701).is_ok());
    }

    #[test]
    fn revocation_is_fail_closed() {
        let mut store = IdentityTrustStore::default();
        let cert = store.enroll("node-a", b"cert-1", 1, 100).unwrap();
        store.revoke_certificate(&cert.fingerprint).unwrap();
        assert!(matches!(
            store.verify_peer("node-a", b"cert-1", 50),
            Err(IdentityError::CertificateRevoked)
        ));
    }

    #[test]
    fn trust_store_round_trips_without_private_keys() {
        let mut store = IdentityTrustStore::default();
        let certificate = store.enroll("node-a", b"cert-1", 1, 100).unwrap();
        store.revoke_certificate(&certificate.fingerprint).unwrap();
        let json = store.to_json().unwrap();
        assert!(!json.contains("PRIVATE KEY"));
        let restored = IdentityTrustStore::from_json(&json).unwrap();
        assert!(restored.is_revoked(&certificate.fingerprint));
    }

    #[test]
    fn malformed_serialized_fingerprint_is_rejected() {
        let json = r#"{
            "identities": {
                "node-a": {
                    "node_id": "node-a",
                    "certificates": [{
                        "fingerprint": "not-a-sha256",
                        "generation": 1,
                        "valid_from_secs": 1,
                        "valid_until_secs": 100
                    }]
                }
            },
            "revoked_fingerprints": []
        }"#;
        assert!(matches!(
            IdentityTrustStore::from_json(json),
            Err(IdentityError::InvalidCertificateFingerprint)
        ));
    }

    #[test]
    fn public_controller_addresses_remain_rejected() {
        let public: SocketAddr = "8.8.8.8:443".parse().unwrap();
        let private: SocketAddr = "127.0.0.1:443".parse().unwrap();
        assert!(validate_private_controller_address(public).is_err());
        assert!(validate_private_controller_address(private).is_ok());
    }

    #[test]
    fn real_loopback_mutual_tls_fixture_passes() {
        let report = run_mtls_fixture().unwrap();
        assert!(report.encrypted_round_trip);
        assert!(report.client_certificate_observed);
        assert!(report.server_certificate_observed);
        assert!(report.node_identity_verified);
        assert!(report.renewal_verified);
        assert!(report.revocation_verified);
    }
}
