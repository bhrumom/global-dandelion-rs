//! TLS 1.3 mutual-authenticated transfer. No custom cryptography, insecure
//! certificate verifier, DNS target expansion, anonymous peer or payload retries.
use crate::{
    Receipt, Transport,
    metered::{Metered, WireBudget, classify},
};
use dandelion_core::{Error, MAX_DATAGRAM_BYTES, Result, targets::AuthorizedPeer};
use rustls::{
    ClientConfig, RootCertStore, ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, ServerName, pem::PemObject},
};
use serde::Serialize;
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinSet,
};
use tokio_rustls::{TlsAcceptor, TlsConnector};
use tokio_util::sync::CancellationToken;
const ALPN: &[u8] = b"global-dandelion/2";
fn config_error(_: impl std::fmt::Display) -> Error {
    Error::Config("invalid TLS credentials or trust configuration".into())
}
fn roots(pem: &[u8]) -> Result<RootCertStore> {
    let mut roots = RootCertStore::empty();
    for cert in CertificateDer::pem_slice_iter(pem) {
        roots
            .add(cert.map_err(config_error)?)
            .map_err(config_error)?;
    }
    if roots.is_empty() {
        return Err(config_error("empty CA"));
    }
    Ok(roots)
}
fn identity(
    cert: &[u8],
    key: &[u8],
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    let certs = CertificateDer::pem_slice_iter(cert)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(config_error)?;
    if certs.is_empty() {
        return Err(config_error("empty identity"));
    }
    Ok((
        certs,
        PrivateKeyDer::from_pem_slice(key).map_err(config_error)?,
    ))
}
pub fn client_config(ca: &[u8], cert: &[u8], key: &[u8]) -> Result<Arc<ClientConfig>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let (certs, key) = identity(cert, key)?;
    let mut cfg = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(config_error)?
        .with_root_certificates(roots(ca)?)
        .with_client_auth_cert(certs, key)
        .map_err(config_error)?;
    cfg.alpn_protocols = vec![ALPN.to_vec()];
    cfg.enable_early_data = false;
    Ok(Arc::new(cfg))
}
pub fn server_config(ca: &[u8], cert: &[u8], key: &[u8]) -> Result<Arc<ServerConfig>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let (certs, key) = identity(cert, key)?;
    let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
        Arc::new(roots(ca)?),
        provider.clone(),
    )
    .build()
    .map_err(config_error)?;
    let mut cfg = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(config_error)?
        .with_client_cert_verifier(verifier)
        .with_single_cert(certs, key)
        .map_err(config_error)?;
    cfg.alpn_protocols = vec![ALPN.to_vec()];
    cfg.max_early_data_size = 0;
    Ok(Arc::new(cfg))
}

#[derive(Debug, Serialize)]
pub struct PeerClock {
    pub authenticated: bool,
    pub peer_unix_ms: u64,
    pub local_unix_ms: u64,
    pub estimated_offset_ms: i64,
    pub round_trip_ms: u64,
    pub uncertainty_ms: u64,
}
fn unix_ms() -> Result<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| Error::Task(e.to_string()))?
        .as_millis()
        .min(u128::from(u64::MAX)) as u64)
}
enum Incoming {
    Complete,
    ClockQuery,
    Payload(u64),
}
pub struct Peers {
    streams: Vec<tokio_rustls::client::TlsStream<Metered<TcpStream>>>,
    timeout: Duration,
}
impl Peers {
    /// Read-only authenticated peer wall-clock comparison, not a UTC authority.
    pub async fn clocks(&mut self, cancel: &CancellationToken) -> Result<Vec<PeerClock>> {
        let mut results = Vec::with_capacity(self.streams.len());
        for stream in &mut self.streams {
            let local = unix_ms()?;
            let started = std::time::Instant::now();
            let query = async {
                stream.write_u32(1).await.map_err(classify)?;
                stream.write_u8(1).await.map_err(classify)?;
                stream.flush().await.map_err(classify)?;
                let mut tag = [0; 4];
                stream.read_exact(&mut tag).await.map_err(classify)?;
                if &tag != b"GTIM" {
                    return Err(Error::Protocol("peer clock receipt mismatch"));
                }
                stream.read_u64().await.map_err(classify)
            };
            let peer = tokio::select! {biased;
                _=cancel.cancelled()=>return Err(Error::Cancelled),
                result=tokio::time::timeout(self.timeout,query)=>result.map_err(|_|Error::VerifierTimeout)??,
            };
            let elapsed = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
            let midpoint = u128::from(local) + u128::from(elapsed) / 2;
            let offset = (i128::from(peer) - midpoint.min(i128::MAX as u128) as i128)
                .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64;
            results.push(PeerClock {
                authenticated: true,
                peer_unix_ms: peer,
                local_unix_ms: local,
                estimated_offset_ms: offset,
                round_trip_ms: elapsed,
                uncertainty_ms: elapsed.div_ceil(2).saturating_add(1),
            });
        }
        Ok(results)
    }
    pub async fn connect(
        peers: &[AuthorizedPeer],
        config: Arc<ClientConfig>,
        budget: WireBudget,
        timeout: Duration,
        attempts: u8,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        if peers.is_empty() || peers.len() > 8 || !(1..=3).contains(&attempts) {
            return Err(Error::Config("TLS peer or retry limits invalid".into()));
        }
        let connector = TlsConnector::from(config);
        let mut streams = Vec::new();
        for peer in peers {
            let name = ServerName::try_from(peer.server_name().to_owned()).map_err(config_error)?;
            let mut last_error = None;
            for attempt in 0..attempts {
                let connect = async {
                    let socket = TcpStream::connect(peer.address()).await.map_err(classify)?;
                    socket.set_nodelay(true)?;
                    let stream = connector
                        .connect(name.clone(), Metered::new(socket, budget.clone()))
                        .await
                        .map_err(classify)?;
                    if stream.get_ref().1.alpn_protocol() != Some(ALPN) {
                        return Err(Error::Protocol("TLS application protocol mismatch"));
                    }
                    Ok(stream)
                };
                let result = tokio::select! {biased;_=cancel.cancelled()=>return Err(Error::Cancelled),result=tokio::time::timeout(timeout,connect)=>result.map_err(|_|Error::VerifierTimeout)?};
                match result {
                    Ok(stream) => {
                        streams.push(stream);
                        last_error = None;
                        break;
                    }
                    Err(e) => {
                        if matches!(
                            e,
                            Error::BudgetExhausted | Error::Config(_) | Error::Protocol(_)
                        ) {
                            return Err(e);
                        }
                        last_error = Some(e);
                    }
                }
                if attempt + 1 < attempts {
                    tokio::select! {biased;_=cancel.cancelled()=>return Err(Error::Cancelled),_=tokio::time::sleep(Duration::from_millis(100<<attempt))=>{}}
                }
            }
            if let Some(error) = last_error {
                return Err(error);
            }
        }
        Ok(Self { streams, timeout })
    }
}
impl Transport for Peers {
    fn wire_cost(&self, packet: &[u8]) -> u64 {
        (packet.len() as u64 + 4) * self.streams.len() as u64
    }
    async fn finish(&mut self, cancel: &CancellationToken) -> Result<()> {
        for stream in &mut self.streams {
            let close = async {
                stream.write_u32(0).await.map_err(classify)?;
                stream.flush().await.map_err(classify)?;
                let mut done = [0; 4];
                stream.read_exact(&mut done).await.map_err(classify)?;
                if &done != b"GDON" {
                    return Err(Error::Protocol("TLS completion receipt mismatch"));
                }
                // Explicit completion receipt; no data replay on socket close.
                Ok(())
            };
            tokio::select! {biased;
                _=cancel.cancelled()=>return Err(Error::Cancelled),
                result=tokio::time::timeout(self.timeout,close)=>result.map_err(|_|Error::VerifierTimeout)??,
            }
        }
        Ok(())
    }
    async fn transmit(&mut self, packet: &[u8], cancel: &CancellationToken) -> Result<Receipt> {
        let decoded = dandelion_packet::decode(packet)?;
        let expected = blake3::hash(packet);
        let mut receipt = Receipt {
            accepted_wire_bytes: 0,
            verified_payload_bytes: 0,
        };
        for stream in &mut self.streams {
            let transfer = async {
                stream
                    .write_u32(packet.len() as u32)
                    .await
                    .map_err(classify)?;
                stream.write_all(packet).await.map_err(classify)?;
                stream.flush().await.map_err(classify)?;
                let mut acknowledgement = [0; 36];
                stream
                    .read_exact(&mut acknowledgement)
                    .await
                    .map_err(classify)?;
                if &acknowledgement[..4] != b"GACK" || &acknowledgement[4..] != expected.as_bytes()
                {
                    return Err(Error::Protocol("authenticated receipt mismatch"));
                }
                Ok(())
            };
            tokio::select! {biased;_=cancel.cancelled()=>return Err(Error::Cancelled),result=tokio::time::timeout(self.timeout,transfer)=>result.map_err(|_|Error::VerifierTimeout)??,}
            receipt.accepted_wire_bytes += packet.len() as u64 + 4;
            receipt.verified_payload_bytes += decoded.payload.len() as u64;
        }
        Ok(receipt)
    }
}

#[derive(Default, Debug, Serialize)]
pub struct ReceiverReport {
    pub sessions: u64,
    pub failed_sessions: u64,
    pub frames: u64,
    pub verified_payload_bytes: u64,
    pub transmitted_ciphertext_bytes: u64,
    pub received_ciphertext_bytes: u64,
    pub cancelled: bool,
}
#[derive(Clone)]
pub struct ReceiveOptions {
    pub max_sessions: usize,
    pub max_connections: Option<u64>,
    pub timeout: Duration,
    pub allowed_clients: Vec<std::net::IpAddr>,
}
/// Accepts at most max_sessions concurrent tasks; JoinSet is drained during the
/// accept loop and all tasks are aborted+joined on cancellation (no detached work).
pub async fn serve(
    listener: TcpListener,
    config: Arc<ServerConfig>,
    budget: WireBudget,
    options: ReceiveOptions,
    cancel: CancellationToken,
) -> Result<ReceiverReport> {
    if !(1..=16).contains(&options.max_sessions) || options.allowed_clients.len() > 64 {
        return Err(Error::Config(
            "receiver session/client limits invalid".into(),
        ));
    }
    let acceptor = TlsAcceptor::from(config);
    let mut tasks = JoinSet::new();
    let mut accepted = 0u64;
    let mut report = ReceiverReport::default();
    loop {
        let full = options.max_connections.is_some_and(|max| accepted >= max);
        if full && tasks.is_empty() {
            break;
        }
        tokio::select! {biased;
            _=cancel.cancelled()=>{report.cancelled=true;break;},
            result=tasks.join_next(),if !tasks.is_empty()=>{
                match result{
                    Some(Ok(Ok((frames,bytes))))=>{report.sessions+=1;report.frames+=frames;report.verified_payload_bytes+=bytes;},
                    Some(Ok(Err(_)))=>report.failed_sessions+=1,
                    Some(Err(e))=>return Err(Error::Task(e.to_string())),None=>{}
                }
            },
            incoming=listener.accept(),if !full && tasks.len()<options.max_sessions=>{
                let (stream,peer)=incoming?;
                if !options.allowed_clients.is_empty()&&!options.allowed_clients.contains(&peer.ip()){drop(stream);continue;}
                accepted+=1;let acceptor=acceptor.clone();let budget=budget.clone();let timeout=options.timeout;
                tasks.spawn(async move{
                    let mut stream=tokio::time::timeout(timeout,acceptor.accept(Metered::new(stream,budget))).await.map_err(|_|Error::VerifierTimeout)?.map_err(classify)?;
                    if stream.get_ref().1.alpn_protocol()!=Some(ALPN){return Err(Error::Protocol("TLS application protocol mismatch"));}
                    let mut buffer=[0;MAX_DATAGRAM_BYTES];let mut frames=0u64;let mut bytes=0u64;
                    loop{
                        let packet=async{
                            let mut header=[0;4];if stream.read(&mut header[..1]).await.map_err(classify)?==0{return Err(Error::Protocol("TLS session ended without completion marker"));}
                            stream.read_exact(&mut header[1..]).await.map_err(classify)?;let len=u32::from_be_bytes(header) as usize;
                            if len==0 {
                                stream.write_all(b"GDON").await.map_err(classify)?;
                                stream.flush().await.map_err(classify)?;return Ok(Incoming::Complete);
                            }

                            if len==1 {
                                if stream.read_u8().await.map_err(classify)?!=1{return Err(Error::Protocol("unknown peer control request"));}
                                stream.write_all(b"GTIM").await.map_err(classify)?;
                                stream.write_u64(unix_ms()?).await.map_err(classify)?;
                                stream.flush().await.map_err(classify)?;return Ok(Incoming::ClockQuery);
                            }
                            if !(dandelion_core::HEADER_BYTES..=MAX_DATAGRAM_BYTES).contains(&len){return Err(Error::Protocol("TLS frame length invalid"));}
                            stream.read_exact(&mut buffer[..len]).await.map_err(classify)?;let frame=dandelion_packet::decode(&buffer[..len])?;
                            let payload=frame.payload.len() as u64;let digest=blake3::hash(&buffer[..len]);
                            stream.write_all(b"GACK").await.map_err(classify)?;stream.write_all(digest.as_bytes()).await.map_err(classify)?;stream.flush().await.map_err(classify)?;
                            Ok(Incoming::Payload(payload))
                        };
                        match tokio::time::timeout(timeout,packet).await.map_err(|_|Error::VerifierTimeout)??{Incoming::Payload(n)=>{frames+=1;bytes+=n;},Incoming::Complete=>break,Incoming::ClockQuery=>{},}
                    }Ok((frames,bytes))
                });
            }
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    let (_, written, received) = budget.totals()?;
    report.transmitted_ciphertext_bytes = written;
    report.received_ciphertext_bytes = received;
    Ok(report)
}
