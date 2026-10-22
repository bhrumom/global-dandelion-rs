//! Managed onboarding belongs to the package, with a private atomic profile.
use dandelion_core::{Error, Result};
use dandelion_network::{Identity, ManagedHttps, Prepared};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn invalid() -> Error {
    Error::Config("private network profile required".into())
}
#[cfg(unix)]
fn private_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let meta = fs::symlink_metadata(path)?;
    if !path.is_absolute()
        || !meta.is_dir()
        || meta.file_type().is_symlink()
        || meta.mode() & 0o077 != 0
    {
        return Err(invalid());
    }
    Ok(())
}
#[cfg(not(unix))]
fn private_directory(_: &Path) -> Result<()> {
    Err(invalid())
}
fn reserve(path: &Path) -> Result<()> {
    dandelion_platform_linux::disk_space(path)?.check(1024 * 1024)
}
#[cfg(unix)]
fn identity(path: &Path) -> Result<Identity> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let key_path = path.join("network-identity.pem");
    match fs::OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(&key_path)
    {
        Ok(file) => {
            let meta = file.metadata()?;
            if !meta.is_file() || meta.len() > 16384 || meta.mode() & 0o077 != 0 {
                return Err(invalid());
            }
            let mut key = String::new();
            file.take(16385).read_to_string(&mut key)?;
            Identity::from_private_pem(&key)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let key = Identity::generate()?;
            reserve(path)?;
            let mut staged = tempfile::NamedTempFile::new_in(path)?;
            staged.write_all(key.private_pem().as_bytes())?;
            staged.as_file().sync_all()?;
            staged.persist_noclobber(key_path).map_err(|_| invalid())?;
            Ok(key)
        }
        Err(_) => Err(invalid()),
    }
}
#[cfg(not(unix))]
fn identity(_: &Path) -> Result<Identity> {
    Err(invalid())
}
async fn probe(prepared: &Prepared) -> Result<Vec<dandelion_core::targets::Peer>> {
    use dandelion_transport::{
        Transport,
        metered::WireBudget,
        tls::{self, Peers},
    };
    let config = tls::client_config(
        prepared.directory.ca_pem.as_bytes(),
        prepared.enrollment.certificate_pem.as_bytes(),
        prepared.identity.private_pem().as_bytes(),
    )?;
    let cancel = tokio_util::sync::CancellationToken::new();
    let budget = WireBudget::new(256 * 1024, 128 * 1024, 64 * 1024)?;
    let mut healthy = Vec::new();
    for peer in prepared.directory.selected_peers() {
        let authorized =
            dandelion_core::targets::authorize(std::slice::from_ref(&peer), true, &[peer.address])?;
        let probe = async {
            let mut connection = Peers::connect(
                &authorized,
                config.clone(),
                budget.clone(),
                Duration::from_secs(2),
                1,
                &cancel,
            )
            .await?;
            let clocks = connection.clocks(&cancel).await?;
            connection.finish(&cancel).await?;
            if clocks.len() != 1 || !clocks[0].authenticated {
                return Err(Error::Protocol("unauthenticated receiver probe"));
            }
            Ok(())
        };
        if matches!(
            tokio::time::timeout(Duration::from_secs(3), probe).await,
            Ok(Ok(()))
        ) {
            healthy.push(peer);
        }
    }
    if healthy.is_empty() {
        return Err(Error::Config(
            "managed receivers are not reachable; local loopback remains available".into(),
        ));
    }
    Ok(healthy)
}
fn write(path: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    reserve(path)?;
    let mut file = tempfile::NamedTempFile::new_in(path)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(path.join(name))
        .map_err(|_| invalid())?;
    Ok(())
}
pub fn prepare(directory: &Path) -> Result<serde_json::Value> {
    private_directory(directory)?;
    reserve(directory)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid())?
        .as_secs();
    let prepared = dandelion_network::prepare(&ManagedHttps::new()?, identity(directory)?, now)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let peers = runtime.block_on(probe(&prepared))?;
    // No credentials or config become active before actual protocol verification.
    reserve(directory)?;
    // A crashed owner may leave a generation behind; refuse unlimited growth.
    let generations = fs::read_dir(directory)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("network-generation-")
        })
        .take(8)
        .count();
    if generations >= 8 {
        return Err(Error::Config("network generation limit; close previous Mini App instances and remove their owned stale profiles".into()));
    }
    let mut builder = tempfile::Builder::new();
    builder.prefix("network-generation-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    let staged = builder.tempdir_in(directory)?;
    private_directory(staged.path())?;
    write(
        staged.path(),
        "ca.pem",
        prepared.directory.ca_pem.as_bytes(),
    )?;
    write(
        staged.path(),
        "client.pem",
        prepared.enrollment.certificate_pem.as_bytes(),
    )?;
    write(
        staged.path(),
        "identity.pem",
        prepared.identity.private_pem().as_bytes(),
    )?;
    let mut config = dandelion_config::Config::default();
    config.scanner.roots.clear();
    config.scanner.allowed_mount_prefixes.clear();
    config.scanner.max_files = 128;
    config.scanner.max_read_bytes = 1024 * 1024 * 1024;
    config.state.directory = directory.join("state");
    config.transport.mode = dandelion_config::Mode::Tls;
    config.transport.tls.ca_file = staged.path().join("ca.pem");
    config.transport.tls.certificate_file = staged.path().join("client.pem");
    config.transport.tls.private_key_file = staged.path().join("identity.pem");
    config.transport.tls.wan_enabled = true;
    config.transport.tls.wan_allowlist = peers.iter().map(|p| p.address).collect();
    config.transport.tls.peers = peers;
    let config = config.validate()?.get().clone();
    write(staged.path(), "service.toml", config.to_toml()?.as_bytes())?;
    let expires = prepared
        .directory
        .nodes
        .iter()
        .filter(|node| {
            config
                .transport
                .tls
                .peers
                .iter()
                .any(|p| p.address == node.address)
        })
        .map(|node| node.ready_until)
        .min()
        .ok_or_else(invalid)?;
    if SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid())?
        .as_secs()
        >= expires
    {
        return Err(Error::Config(
            "receiver directory expired during verification".into(),
        ));
    }
    let result = serde_json::json!({"config_path": staged.path().join("service.toml"), "expires_unix_seconds": expires, "receivers": config.transport.tls.peers.len(), "authenticated": true});
    let _owned_generation = staged.keep();
    Ok(result)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    #[test]
    fn identity_is_private_stable_and_rejects_symlinks() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(private_directory(root.path()).is_err());
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        private_directory(root.path()).unwrap();
        let first = identity(root.path()).unwrap();
        assert_eq!(first.id(), identity(root.path()).unwrap().id());
        fs::set_permissions(
            root.path().join("network-identity.pem"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(identity(root.path()).is_err());
        let other = tempfile::tempdir().unwrap();
        symlink(
            root.path().join("network-identity.pem"),
            other.path().join("network-identity.pem"),
        )
        .unwrap();
        assert!(identity(other.path()).is_err());
    }
    #[tokio::test]
    async fn live_probe_checks_mutual_tls_and_gdan_without_payload() {
        use dandelion_network::{Authority, Directory, Receiver};
        use dandelion_transport::{
            metered::WireBudget,
            tls::{ReceiveOptions, serve, server_config},
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let ca = Authority::generate(now as i64).unwrap();
        let server = Identity::generate().unwrap();
        let client = Identity::generate().unwrap();
        let cert = ca
            .issue_receiver(vec!["localhost".into()], &server, now as i64)
            .unwrap();
        let config = server_config(
            ca.ca_pem().as_bytes(),
            cert.as_bytes(),
            server.private_pem().as_bytes(),
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let prepared = Prepared {
            directory: Directory {
                schema_version: 1,
                expires_unix_seconds: now + 300,
                ca_pem: ca.ca_pem(),
                nodes: vec![Receiver {
                    id: "test".into(),
                    address,
                    server_name: "localhost".into(),
                    region: "fixture".into(),
                    ready_until: now + 300,
                }],
            },
            enrollment: ca.issue_client(&client.csr().unwrap(), now as i64).unwrap(),
            identity: client,
        };
        let receiver = serve(
            listener,
            config,
            WireBudget::new(256 * 1024, 128 * 1024, 64 * 1024).unwrap(),
            ReceiveOptions {
                max_sessions: 1,
                max_connections: Some(1),
                timeout: Duration::from_secs(5),
                allowed_clients: vec![address.ip()],
            },
            tokio_util::sync::CancellationToken::new(),
        );
        let (result, report) = tokio::join!(probe(&prepared), receiver);
        assert_eq!(result.unwrap()[0].address, address);
        let report = report.unwrap();
        assert_eq!(report.sessions, 1);
        assert_eq!(report.verified_payload_bytes, 0);
        assert_eq!(report.failed_sessions, 0);
    }
}
