//! Bounded managed receiver discovery and per-device certificate enrollment.
//! Private keys are generated locally; CSR requests cannot grant CA/server rights.
use dandelion_core::{
    Error, Result,
    targets::{NetworkClass, Peer, TargetPool, TargetPoolEntry},
};
use rcgen::{
    CertificateParams, CertificateSigningRequestParams, CertifiedIssuer, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose, PublicKeyData,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, io::Read, net::SocketAddr, time::Duration};
use time::OffsetDateTime;

// This immutable resource is reviewed and delivered with the Mini App binary.
// An empty catalogue is explicitly unconfigured, never substituted with random IPs.
pub const BUILTIN_RECEIVERS: &[u8] = include_bytes!("../receivers.json");
pub const ENROLL_URL: &str = "https://api.ombhrum.com/v1/dandelion/enroll";
pub const MAX_DOCUMENT: usize = 65536;
pub const CLIENT_TTL: i64 = 7 * 86400;
fn invalid(message: &str) -> Error {
    Error::Config(format!("network onboarding: {message}"))
}
fn crypto(_: impl std::fmt::Display) -> Error {
    invalid("certificate operation failed")
}
fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}
fn bounded_pem(value: &str, kind: &str) -> bool {
    value.len() <= 16384
        && !value.contains('\0')
        && value.trim().starts_with(&format!("-----BEGIN {kind}-----"))
        && value.trim().ends_with(&format!("-----END {kind}-----"))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receiver {
    pub id: String,
    pub address: SocketAddr,
    pub server_name: String,
    pub region: String,
    pub ready_until: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Directory {
    pub schema_version: u32,
    pub expires_unix_seconds: u64,
    pub ca_pem: String,
    pub nodes: Vec<Receiver>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BuiltinCatalogue {
    schema_version: u32,
    ca_pem: String,
    nodes: Vec<BuiltinReceiver>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BuiltinReceiver {
    id: String,
    address: SocketAddr,
    server_name: String,
    region: String,
}
impl Directory {
    /// Seeds and trust are shipped in the signed package. A local validity window
    /// bounds prepared profiles; only a successful TLS protocol probe proves live readiness.
    pub fn from_builtin(raw: &[u8], now: u64) -> Result<Self> {
        if raw.len() > MAX_DOCUMENT {
            return Err(invalid("builtin catalogue limit"));
        }
        let seeds: BuiltinCatalogue =
            serde_json::from_slice(raw).map_err(|_| invalid("builtin catalogue schema"))?;
        let expires = now
            .checked_add(900)
            .ok_or_else(|| invalid("catalogue clock"))?;
        let directory = Self {
            schema_version: seeds.schema_version,
            expires_unix_seconds: expires,
            ca_pem: seeds.ca_pem,
            nodes: seeds
                .nodes
                .into_iter()
                .map(|node| Receiver {
                    id: node.id,
                    address: node.address,
                    server_name: node.server_name,
                    region: node.region,
                    ready_until: expires,
                })
                .collect(),
        };
        Self::parse(&serde_json::to_vec(&directory).map_err(crypto)?, now)
    }

    pub fn parse(raw: &[u8], now: u64) -> Result<Self> {
        if raw.len() > MAX_DOCUMENT {
            return Err(invalid("directory limit"));
        }
        let value: Self = serde_json::from_slice(raw).map_err(|_| invalid("directory schema"))?;
        if value.schema_version != 1
            || value.expires_unix_seconds <= now
            || value.expires_unix_seconds - now > 900
            || value.nodes.is_empty()
            || value.nodes.len() > 32
            || !bounded_pem(&value.ca_pem, "CERTIFICATE")
        {
            return Err(invalid("directory expiry, CA or node bounds"));
        }
        let mut identities = BTreeSet::new();
        let mut addresses = BTreeSet::new();
        for node in &value.nodes {
            if !id(&node.id)
                || !identities.insert(node.id.clone())
                || !addresses.insert(node.address)
                || node.server_name.len() > 253
                || node.region.len() > 80
                || node.region.chars().any(char::is_control)
                || node.ready_until <= now
                || node.ready_until > value.expires_unix_seconds
            {
                return Err(invalid("receiver metadata or freshness"));
            }
            let prefix = if node.address.is_ipv4() { 32 } else { 128 };
            TargetPool::new(&[TargetPoolEntry {
                cidr: format!("{}/{prefix}", node.address.ip()),
                port: node.address.port(),
                server_name: node.server_name.clone(),
            }])?;
            let peer = Peer {
                address: node.address,
                server_name: node.server_name.clone(),
                class: NetworkClass::Wan,
                authorized: true,
            };
            dandelion_core::targets::authorize(&[peer], true, &[node.address])?;
        }
        Ok(value)
    }
    pub fn fingerprint(&self) -> Result<String> {
        Ok(blake3::hash(&serde_json::to_vec(self).map_err(crypto)?)
            .to_hex()
            .to_string())
    }
    pub fn selected_peers(&self) -> Vec<Peer> {
        self.nodes
            .iter()
            .take(8)
            .map(|node| Peer {
                address: node.address,
                server_name: node.server_name.clone(),
                class: NetworkClass::Wan,
                authorized: true,
            })
            .collect()
    }
}

pub struct Identity {
    key: KeyPair,
}
impl Identity {
    pub fn generate() -> Result<Self> {
        Ok(Self {
            key: KeyPair::generate_for(&rcgen::PKCS_ED25519).map_err(crypto)?,
        })
    }
    pub fn from_private_pem(pem: &str) -> Result<Self> {
        if pem.len() > 16384 {
            return Err(invalid("private identity limit"));
        }
        Ok(Self {
            key: KeyPair::from_pem(pem).map_err(crypto)?,
        })
    }
    pub fn private_pem(&self) -> String {
        self.key.serialize_pem()
    }
    pub fn id(&self) -> String {
        blake3::hash(self.key.der_bytes()).to_hex().to_string()
    }
    #[allow(clippy::field_reassign_with_default)]
    pub fn csr(&self) -> Result<String> {
        let mut params = CertificateParams::default();
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, self.id());
        params
            .serialize_request(&self.key)
            .map_err(crypto)?
            .pem()
            .map_err(crypto)
    }
}

fn validity(params: &mut CertificateParams, now: i64, ttl: i64) -> Result<()> {
    params.not_before = OffsetDateTime::from_unix_timestamp(
        now.checked_sub(60)
            .ok_or_else(|| invalid("certificate clock"))?,
    )
    .map_err(crypto)?;
    params.not_after = OffsetDateTime::from_unix_timestamp(
        now.checked_add(ttl)
            .ok_or_else(|| invalid("certificate clock"))?,
    )
    .map_err(crypto)?;
    Ok(())
}
/// Operator-owned network authority; never shipped as a client private key.
pub struct Authority {
    issuer: CertifiedIssuer<'static, KeyPair>,
}
impl Authority {
    #[allow(clippy::field_reassign_with_default)]
    pub fn generate(now: i64) -> Result<Self> {
        let mut params = CertificateParams::default();
        params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Constrained(0));
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, "Global Dandelion receiver network");
        validity(&mut params, now, 365 * 86400)?;
        Ok(Self {
            issuer: CertifiedIssuer::self_signed(
                params,
                KeyPair::generate_for(&rcgen::PKCS_ED25519).map_err(crypto)?,
            )
            .map_err(crypto)?,
        })
    }
    pub fn ca_pem(&self) -> String {
        self.issuer.pem()
    }
    pub fn private_pem(&self) -> String {
        self.issuer.key().serialize_pem()
    }
    #[allow(clippy::field_reassign_with_default)]
    pub fn issue_client(&self, csr: &str, now: i64) -> Result<Enrollment> {
        if !bounded_pem(csr, "CERTIFICATE REQUEST") {
            return Err(invalid("CSR limit or format"));
        }
        let mut request = CertificateSigningRequestParams::from_pem(csr).map_err(crypto)?;
        let identity = blake3::hash(request.public_key.der_bytes())
            .to_hex()
            .to_string();
        // Discard every caller-controlled extension and subject. In particular a
        // CSR may never obtain CA rights, serverAuth, alternate names or custom OIDs.
        request.params = CertificateParams::default();
        request.params.is_ca = IsCa::ExplicitNoCa;
        request.params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        request.params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        request.params.distinguished_name = DistinguishedName::new();
        request
            .params
            .distinguished_name
            .push(DnType::CommonName, &identity);
        validity(&mut request.params, now, CLIENT_TTL)?;
        let certificate_pem = request.signed_by(&self.issuer).map_err(crypto)?.pem();
        Ok(Enrollment {
            schema_version: 1,
            identity,
            certificate_pem,
            expires_unix_seconds: u64::try_from(now + CLIENT_TTL).map_err(crypto)?,
        })
    }
    /// Managed receiver server certificate, generated only by its operator.
    pub fn issue_receiver(&self, names: Vec<String>, key: &Identity, now: i64) -> Result<String> {
        if names.is_empty() || names.len() > 8 || names.iter().any(|name| name.len() > 253) {
            return Err(invalid("receiver names"));
        }
        let mut params = CertificateParams::new(names).map_err(crypto)?;
        params.is_ca = IsCa::ExplicitNoCa;
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        validity(&mut params, now, 30 * 86400)?;
        Ok(params
            .signed_by(&key.key, &self.issuer)
            .map_err(crypto)?
            .pem())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Enrollment {
    pub schema_version: u32,
    pub identity: String,
    pub certificate_pem: String,
    pub expires_unix_seconds: u64,
}
impl Enrollment {
    /// Validate client usage, issuer, validity and possession of the matching key.
    /// Directory metadata alone is never proof of a valid device identity.
    pub fn verify(&self, ca: &str, identity: &Identity, now: u64) -> Result<()> {
        use rustls::pki_types::{CertificateDer, PrivateKeyDer, UnixTime, pem::PemObject};
        use std::sync::Arc;
        let certs = CertificateDer::pem_slice_iter(self.certificate_pem.as_bytes())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(crypto)?;
        if certs.len() != 1 {
            return Err(invalid("single client certificate required"));
        }
        let mut roots = rustls::RootCertStore::empty();
        for root in CertificateDer::pem_slice_iter(ca.as_bytes()) {
            roots.add(root.map_err(crypto)?).map_err(crypto)?;
        }
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
            Arc::new(roots.clone()),
            provider.clone(),
        )
        .build()
        .map_err(crypto)?;
        verifier
            .verify_client_cert(
                &certs[0],
                &[],
                UnixTime::since_unix_epoch(Duration::from_secs(now)),
            )
            .map_err(crypto)?;
        let key =
            PrivateKeyDer::from_pem_slice(identity.private_pem().as_bytes()).map_err(crypto)?;
        rustls::ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(crypto)?
            .with_root_certificates(roots)
            .with_client_auth_cert(certs, key)
            .map_err(crypto)?;
        Ok(())
    }
    pub fn parse(raw: &[u8], identity: &Identity, now: u64) -> Result<Self> {
        if raw.len() > MAX_DOCUMENT {
            return Err(invalid("enrollment limit"));
        }
        let value: Self = serde_json::from_slice(raw).map_err(|_| invalid("enrollment schema"))?;
        if value.schema_version != 1
            || value.identity != identity.id()
            || value.expires_unix_seconds <= now
            || value.expires_unix_seconds - now > CLIENT_TTL as u64
            || !bounded_pem(&value.certificate_pem, "CERTIFICATE")
        {
            return Err(invalid("enrollment identity or expiry"));
        }
        Ok(value)
    }
}

/// Inject the control transport in tests; only this HTTPS implementation is
/// wired in production. It never sees the client's private identity material.
pub trait ControlPlane {
    fn directory(&self, now: u64) -> Result<Vec<u8>>;
    fn enroll(&self, csr: &str) -> Result<Vec<u8>>;
}
pub struct ManagedHttps {
    client: reqwest::blocking::Client,
}
impl ManagedHttps {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: reqwest::blocking::Client::builder()
                .https_only(true)
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(4))
                .timeout(Duration::from_secs(8))
                .build()
                .map_err(|_| invalid("HTTPS client unavailable"))?,
        })
    }
    fn read(response: reqwest::blocking::Response) -> Result<Vec<u8>> {
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|n| n > MAX_DOCUMENT as u64)
        {
            return Err(invalid("control service unavailable or oversized"));
        }
        let mut bytes = Vec::new();
        response
            .take((MAX_DOCUMENT + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("control response failed"))?;
        if bytes.len() > MAX_DOCUMENT {
            return Err(invalid("control response limit"));
        }
        Ok(bytes)
    }
}
impl ControlPlane for ManagedHttps {
    fn directory(&self, now: u64) -> Result<Vec<u8>> {
        serde_json::to_vec(&Directory::from_builtin(BUILTIN_RECEIVERS, now)?).map_err(crypto)
    }
    fn enroll(&self, csr: &str) -> Result<Vec<u8>> {
        if !bounded_pem(csr, "CERTIFICATE REQUEST") {
            return Err(invalid("CSR limit"));
        }
        let payload =
            serde_json::to_vec(&serde_json::json!({ "schema_version": 1, "csr_pem": csr }))
                .map_err(crypto)?;
        Self::read(
            self.client
                .post(ENROLL_URL)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(payload)
                .send()
                .map_err(|_| invalid("enrollment unreachable"))?,
        )
    }
}

pub struct Prepared {
    pub directory: Directory,
    pub enrollment: Enrollment,
    pub identity: Identity,
}
pub fn prepare(control: &impl ControlPlane, identity: Identity, now: u64) -> Result<Prepared> {
    let directory = Directory::parse(&control.directory(now)?, now)?;
    let enrollment = Enrollment::parse(&control.enroll(&identity.csr()?)?, &identity, now)?;
    enrollment.verify(&directory.ca_pem, &identity, now)?;
    Ok(Prepared {
        directory,
        enrollment,
        identity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const NOW: u64 = 1_800_000_000;
    fn directory(ca: String) -> Directory {
        Directory {
            schema_version: 1,
            expires_unix_seconds: NOW + 300,
            ca_pem: ca,
            nodes: vec![Receiver {
                id: "managed-1".into(),
                address: "203.0.113.10:20443".parse().unwrap(),
                server_name: "receiver.example.invalid".into(),
                region: "test".into(),
                ready_until: NOW + 120,
            }],
        }
    }
    fn bytes<T: Serialize>(value: &T) -> Vec<u8> {
        serde_json::to_vec(value).unwrap()
    }
    #[test]
    fn builtin_seeds_are_package_owned_and_never_invented() {
        let ca = Authority::generate(NOW as i64).unwrap();
        let seeds = serde_json::json!({"schema_version":1,"ca_pem":ca.ca_pem(),"nodes":[{
            "id":"builtin-1","address":"203.0.113.10:20443","server_name":"receiver.example.invalid","region":"fixture"
        }]});
        let directory = Directory::from_builtin(&bytes(&seeds), NOW).unwrap();
        assert_eq!(
            directory.selected_peers()[0].address,
            "203.0.113.10:20443".parse().unwrap()
        );
        assert_eq!(directory.expires_unix_seconds, NOW + 900);
        assert!(
            Directory::from_builtin(br#"{"schema_version":1,"ca_pem":"","nodes":[]}"#, NOW)
                .is_err()
        );
        assert!(Directory::from_builtin(&bytes(&seeds), u64::MAX).is_err());
    }
    #[test]
    fn directory_rejects_stale_private_duplicate_and_oversized_nodes() {
        let ca = Authority::generate(NOW as i64).unwrap();
        let base = directory(ca.ca_pem());
        assert!(Directory::parse(&bytes(&base), NOW).is_ok());
        for address in [
            "127.0.0.1:443",
            "10.0.0.1:443",
            "169.254.1.1:443",
            "[::1]:443",
            "[fc00::1]:443",
            "0.0.0.0:443",
        ] {
            let mut value = base.clone();
            value.nodes[0].address = address.parse().unwrap();
            assert!(Directory::parse(&bytes(&value), NOW).is_err(), "{address}");
        }
        let mut stale = base.clone();
        stale.nodes[0].ready_until = NOW;
        assert!(Directory::parse(&bytes(&stale), NOW).is_err());
        let mut repeated = base.clone();
        repeated.nodes.push(base.nodes[0].clone());
        assert!(Directory::parse(&bytes(&repeated), NOW).is_err());
        assert!(Directory::parse(&vec![b' '; MAX_DOCUMENT + 1], NOW).is_err());
        assert!(Directory::parse(&bytes(&base), NOW + 301).is_err());
    }
    #[test]
    fn enrollment_proves_key_issuer_client_usage_and_expiry() {
        let ca = Authority::generate(NOW as i64).unwrap();
        let identity = Identity::generate().unwrap();
        let enrollment = ca
            .issue_client(&identity.csr().unwrap(), NOW as i64)
            .unwrap();
        assert_eq!(enrollment.identity, identity.id());
        enrollment.verify(&ca.ca_pem(), &identity, NOW).unwrap();
        let another = Identity::generate().unwrap();
        assert!(enrollment.verify(&ca.ca_pem(), &another, NOW).is_err());
        let other_ca = Authority::generate(NOW as i64).unwrap();
        assert!(
            enrollment
                .verify(&other_ca.ca_pem(), &identity, NOW)
                .is_err()
        );
        assert!(
            enrollment
                .verify(&ca.ca_pem(), &identity, NOW + CLIENT_TTL as u64 + 1)
                .is_err()
        );
        let mut server = enrollment.clone();
        server.certificate_pem = ca
            .issue_receiver(
                vec!["receiver.example.invalid".into()],
                &identity,
                NOW as i64,
            )
            .unwrap();
        assert!(server.verify(&ca.ca_pem(), &identity, NOW).is_err());
        let mut metadata = enrollment.clone();
        metadata.identity = another.id();
        assert!(Enrollment::parse(&bytes(&metadata), &identity, NOW).is_err());
        let restored = Identity::from_private_pem(&identity.private_pem()).unwrap();
        assert_eq!(restored.id(), identity.id());
    }
    #[test]
    fn malicious_csr_cannot_escalate_to_ca_or_receiver() {
        let ca = Authority::generate(NOW as i64).unwrap();
        let identity = Identity::generate().unwrap();
        let mut params = CertificateParams::new(vec!["attacker.example.invalid".into()]).unwrap();
        params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let csr = params
            .serialize_request(&identity.key)
            .unwrap()
            .pem()
            .unwrap();
        // Unsupported privilege extensions may be rejected outright. If parsed,
        // the signer must replace all requested privileges with client-only ones.
        if let Ok(enrollment) = ca.issue_client(&csr, NOW as i64) {
            enrollment.verify(&ca.ca_pem(), &identity, NOW).unwrap();
            use rustls::client::danger::ServerCertVerifier;
            use rustls::pki_types::{CertificateDer, ServerName, UnixTime, pem::PemObject};
            use std::sync::Arc;
            let mut roots = rustls::RootCertStore::empty();
            roots
                .add(CertificateDer::from_pem_slice(ca.ca_pem().as_bytes()).unwrap())
                .unwrap();
            let verifier = rustls::client::WebPkiServerVerifier::builder_with_provider(
                Arc::new(roots),
                Arc::new(rustls::crypto::ring::default_provider()),
            )
            .build()
            .unwrap();
            assert!(
                verifier
                    .verify_server_cert(
                        &CertificateDer::from_pem_slice(enrollment.certificate_pem.as_bytes())
                            .unwrap(),
                        &[],
                        &ServerName::try_from("attacker.example.invalid").unwrap(),
                        &[],
                        UnixTime::since_unix_epoch(Duration::from_secs(NOW))
                    )
                    .is_err()
            );
        }
    }
    struct Fixture {
        ca: Authority,
    }
    impl ControlPlane for Fixture {
        fn directory(&self, _: u64) -> Result<Vec<u8>> {
            Ok(bytes(&directory(self.ca.ca_pem())))
        }
        fn enroll(&self, csr: &str) -> Result<Vec<u8>> {
            Ok(bytes(&self.ca.issue_client(csr, NOW as i64)?))
        }
    }
    #[test]
    fn bootstrap_sends_only_csr_and_rejects_forged_identity() {
        let fixture = Fixture {
            ca: Authority::generate(NOW as i64).unwrap(),
        };
        let prepared = prepare(&fixture, Identity::generate().unwrap(), NOW).unwrap();
        assert_eq!(prepared.directory.selected_peers().len(), 1);
        assert!(!prepared.identity.csr().unwrap().contains("PRIVATE KEY"));
        struct Forged<'a>(&'a Fixture);
        impl ControlPlane for Forged<'_> {
            fn directory(&self, _: u64) -> Result<Vec<u8>> {
                self.0.directory(NOW)
            }
            fn enroll(&self, csr: &str) -> Result<Vec<u8>> {
                let mut response: Enrollment =
                    serde_json::from_slice(&self.0.enroll(csr)?).unwrap();
                response.certificate_pem = self
                    .0
                    .ca
                    .issue_client(&Identity::generate()?.csr()?, NOW as i64)?
                    .certificate_pem;
                Ok(bytes(&response))
            }
        }
        assert!(prepare(&Forged(&fixture), Identity::generate().unwrap(), NOW).is_err());
    }
}
