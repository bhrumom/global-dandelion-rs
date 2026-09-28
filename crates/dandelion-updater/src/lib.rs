//! Verify an independently trusted Ed25519 release manifest, then stage the
//! exact signed artifact. No code execution or replacement of the running binary.
use dandelion_core::{Error, Result, storage::WriteGuard};
use ring::{digest, signature};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
const MAX_MANIFEST: usize = 64 * 1024;
const MAX_ARTIFACT: u64 = 256 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub version: String,
    pub target: String,
    pub size: u64,
    pub sha256: String,
    pub expires_unix_seconds: u64,
    pub artifact_url: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Verified {
    manifest: Manifest,
    raw: Vec<u8>,
    signature: Vec<u8>,
    key_fingerprint: String,
}
impl Verified {
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn failure(message: &str) -> Error {
    Error::Config(format!("update: {message}"))
}

pub fn verify(
    raw: &[u8],
    sig: &[u8],
    public_key: &[u8],
    current_version: &str,
    target: &str,
    now: u64,
    allow_rollback: bool,
) -> Result<Verified> {
    if raw.len() > MAX_MANIFEST || sig.len() != 64 || public_key.len() != 32 {
        return Err(failure("manifest/signature/key length invalid"));
    }
    signature::UnparsedPublicKey::new(&signature::ED25519, public_key)
        .verify(raw, sig)
        .map_err(|_| failure("manifest signature invalid"))?;
    let manifest: Manifest =
        serde_json::from_slice(raw).map_err(|_| failure("signed manifest schema invalid"))?;
    let version = semver::Version::parse(&manifest.version)
        .map_err(|_| failure("invalid release version"))?;
    let current =
        semver::Version::parse(current_version).map_err(|_| failure("invalid current version"))?;
    if manifest.schema_version != 1
        || manifest.target != target
        || manifest.expires_unix_seconds <= now
        || manifest.size == 0
        || manifest.size > MAX_ARTIFACT
    {
        return Err(failure(
            "schema, architecture, expiry, or artifact size rejected",
        ));
    }
    if !version.pre.is_empty()
        || !version.build.is_empty()
        || (!allow_rollback && version <= current)
    {
        return Err(failure(
            "stable updates require a newer non-prerelease version; rollback requires explicit consent",
        ));
    }
    if manifest.sha256.len() != 64 || !manifest.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(failure("invalid SHA-256 digest"));
    }
    if let Some(url) = &manifest.artifact_url {
        trusted_https_url(url)?;
    }
    Ok(Verified {
        manifest,
        raw: raw.to_vec(),
        signature: sig.to_vec(),
        key_fingerprint: hex(digest::digest(&digest::SHA256, public_key).as_ref()),
    })
}

/// Validate the URL before network access; no credentials, fragments or redirects.
pub fn trusted_https_url(url: &str) -> Result<reqwest::Url> {
    let parsed = reqwest::Url::parse(url).map_err(|_| failure("invalid HTTPS URL"))?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
        || url.len() > 4096
    {
        return Err(failure(
            "HTTPS origin must not contain credentials or a fragment",
        ));
    }
    Ok(parsed)
}
fn client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .pool_max_idle_per_host(0)
        .no_proxy()
        .user_agent(concat!("dandelion/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| failure("HTTPS client initialization failed"))
}
/// Caller chose this public HTTPS origin. Responses are bounded before parsing.
pub fn fetch_small(url: &str, max_bytes: usize) -> Result<Vec<u8>> {
    if max_bytes > MAX_MANIFEST {
        return Err(failure("metadata download cap exceeds 64 KiB"));
    }
    let url = trusted_https_url(url)?;
    let response = client()?
        .get(url)
        .send()
        .map_err(|_| failure("metadata request failed"))?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|n| n > max_bytes as u64)
    {
        return Err(failure("metadata HTTP status/size rejected"));
    }
    let mut bytes = Vec::new();
    response
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        return Err(failure("metadata body exceeds limit"));
    }
    Ok(bytes)
}
pub fn read_small(path: &Path, max_bytes: usize) -> Result<Vec<u8>> {
    let file = File::open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > max_bytes as u64 {
        return Err(failure("input is not a bounded regular file"));
    }
    let mut bytes = Vec::new();
    file.take(max_bytes as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        return Err(failure("input grew past size limit"));
    }
    Ok(bytes)
}
pub fn now() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| failure("clock precedes Unix epoch"))?
        .as_secs())
}

#[derive(Debug, Serialize)]
pub struct Staged {
    pub version: String,
    pub target: String,
    pub directory: PathBuf,
    pub sha256: String,
    pub activated: bool,
}
/// No writes to the active executable. The final directory is an immutable
/// version-specific bundle; an existing bundle is never overwritten.
pub fn stage<R: Read, G: WriteGuard>(
    verified: &Verified,
    reader: R,
    root: &Path,
    guard: &G,
) -> Result<Staged> {
    let metadata = fs::symlink_metadata(root)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o022 != 0
    {
        return Err(failure(
            "staging root must be a real directory not writable by group/others",
        ));
    }
    let root = fs::canonicalize(root)?;
    let manifest = &verified.manifest;
    let needed = manifest
        .size
        .checked_add(1024 * 1024)
        .ok_or_else(|| failure("staging estimate overflow"))?;
    guard.check(&root, needed)?;
    let name = format!("dandelion-{}-{}", manifest.version, manifest.target);
    if name.len() > 200
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
    {
        return Err(failure("invalid staging name"));
    }
    let destination = root.join(&name);
    if destination.exists() {
        return Err(failure("version already staged; refusing overwrite"));
    }
    let temp = tempfile::tempdir_in(&root)?;
    let mut artifact = File::create(temp.path().join("artifact"))?;
    let mut hasher = digest::Context::new(&digest::SHA256);
    let mut read = reader.take(manifest.size + 1);
    let mut bytes = 0u64;
    let mut buffer = [0; 64 * 1024];
    loop {
        let n = read.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        if bytes > manifest.size {
            return Err(failure("artifact exceeds signed size"));
        }
        guard.check(&root, (manifest.size - bytes).saturating_add(1024 * 1024))?;
        hasher.update(&buffer[..n]);
        artifact.write_all(&buffer[..n])?;
    }
    let hash = hex(hasher.finish().as_ref());
    if bytes != manifest.size || !hash.eq_ignore_ascii_case(&manifest.sha256) {
        return Err(failure("artifact digest or exact length mismatch"));
    }
    artifact.sync_all()?;
    for (file, data) in [
        ("manifest.json", verified.raw.as_slice()),
        ("manifest.sig", verified.signature.as_slice()),
        (
            "trusted-key-sha256.txt",
            verified.key_fingerprint.as_bytes(),
        ),
    ] {
        let mut out = File::create(temp.path().join(file))?;
        out.write_all(data)?;
        out.sync_all()?;
    }
    File::open(temp.path())?.sync_all()?;
    // The parent is operator-owned; the writer lock serializes bundle promotion.
    let lock_path = root.join(".dandelion-update.lock");
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;
    lock.try_lock()
        .map_err(|_| failure("another update is being staged"))?;
    if destination.exists() {
        return Err(failure("version already staged"));
    }
    fs::rename(temp.path(), &destination)?;
    File::open(&root)?.sync_all()?;
    Ok(Staged {
        version: manifest.version.clone(),
        target: manifest.target.clone(),
        directory: destination,
        sha256: hash,
        activated: false,
    })
}
pub fn stage_download<G: WriteGuard>(
    verified: &Verified,
    root: &Path,
    allowed_host: &str,
    guard: &G,
) -> Result<Staged> {
    let url = trusted_https_url(
        verified
            .manifest
            .artifact_url
            .as_deref()
            .ok_or_else(|| failure("signed artifact URL missing"))?,
    )?;
    if url.host_str() != Some(allowed_host) {
        return Err(failure(
            "artifact host is outside the explicit origin allowlist",
        ));
    }
    guard.check(root, verified.manifest.size + 1024 * 1024)?;
    let response = client()?
        .get(url)
        .send()
        .map_err(|_| failure("artifact request failed"))?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|n| n != verified.manifest.size)
    {
        return Err(failure("artifact HTTP status/length rejected"));
    }
    stage(verified, response, root, guard)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::KeyPair;
    struct Guard;
    impl WriteGuard for Guard {
        fn check(&self, _: &Path, _: u64) -> Result<()> {
            Ok(())
        }
    }
    fn signed() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let key = signature::Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let m = Manifest {
            schema_version: 1,
            version: "0.2.0".into(),
            target: "x86_64-unknown-linux-gnu".into(),
            size: 3,
            sha256: hex(digest::digest(&digest::SHA256, b"abc").as_ref()),
            expires_unix_seconds: 9999999999,
            artifact_url: None,
        };
        let raw = serde_json::to_vec(&m).unwrap();
        let sig = key.sign(&raw).as_ref().to_vec();
        (raw, sig, key.public_key().as_ref().to_vec())
    }
    #[test]
    fn signature_version_arch_expiry_are_mandatory() {
        let (raw, sig, key) = signed();
        assert!(
            verify(
                &raw,
                &sig,
                &key,
                "0.1.0",
                "x86_64-unknown-linux-gnu",
                1,
                false
            )
            .is_ok()
        );
        let mut bad = raw.clone();
        bad[0] ^= 1;
        assert!(
            verify(
                &bad,
                &sig,
                &key,
                "0.1.0",
                "x86_64-unknown-linux-gnu",
                1,
                false
            )
            .is_err()
        );
        assert!(
            verify(
                &raw,
                &sig,
                &key,
                "0.3.0",
                "x86_64-unknown-linux-gnu",
                1,
                false
            )
            .is_err()
        );
        assert!(
            verify(
                &raw,
                &sig,
                &key,
                "0.1.0",
                "aarch64-unknown-linux-gnu",
                1,
                false
            )
            .is_err()
        );
        assert!(
            verify(
                &raw,
                &sig,
                &key,
                "0.1.0",
                "x86_64-unknown-linux-gnu",
                9999999999,
                false
            )
            .is_err()
        );
    }
    #[test]
    fn staging_is_atomic_exact_and_no_overwrite() {
        let (raw, sig, key) = signed();
        let v = verify(
            &raw,
            &sig,
            &key,
            "0.1.0",
            "x86_64-unknown-linux-gnu",
            1,
            false,
        )
        .unwrap();
        let d = tempfile::tempdir().unwrap();
        assert!(stage(&v, &b"bad"[..], d.path(), &Guard).is_err());
        assert!(stage(&v, &b"abcd"[..], d.path(), &Guard).is_err());
        let result = stage(&v, &b"abc"[..], d.path(), &Guard).unwrap();
        assert!(!result.activated);
        assert_eq!(fs::read(result.directory.join("artifact")).unwrap(), b"abc");
        assert!(stage(&v, &b"abc"[..], d.path(), &Guard).is_err());
    }
    #[test]
    fn unsafe_download_urls_rejected_without_network() {
        for url in [
            "http://example.com/file",
            "https://user:pass@example.com/file",
            "file:///tmp/x",
            "https://example.com/a#part",
        ] {
            assert!(trusted_https_url(url).is_err());
        }
    }
}
