//! Explicit NetworkManager hotspot lifecycle with ownership journal and rollback.
//! No shell strings, hidden sudo, regulatory overrides or password command args.
use dandelion_core::{
    Error, Result,
    process::{ProcessRunner, Program, Request},
    storage::WriteGuard,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub interface: String,
    pub ssid: String,
    pub band: String,
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        if self.interface.is_empty()
            || self.interface.len() > 15
            || !self
                .interface
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            || self.ssid.is_empty()
            || self.ssid.len() > 32
            || self.ssid.starts_with('-')
            || self.ssid.chars().any(char::is_control)
            || !matches!(self.band.as_str(), "a" | "bg")
        {
            return Err(Error::Config(
                "hotspot requires a valid interface, 1..32-byte SSID and band a or bg".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Serialize)]
pub struct Plan {
    pub settings: Settings,
    pub requires_explicit_apply: bool,
    pub password_on_argv: bool,
    pub auto_connect: bool,
    pub regulatory_changes: bool,
    pub effect: String,
}
pub fn plan(settings: Settings) -> Result<Plan> {
    settings.validate()?;
    Ok(Plan{settings,requires_explicit_apply:true,password_on_argv:false,auto_connect:false,regulatory_changes:false,effect:"Creates an owned temporary WPA-PSK AP profile; activating it may replace this interface's current connection. Stop restores the previous profile when still appropriate.".into()})
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Journal {
    pub schema_version: u32,
    pub settings: Settings,
    pub uuid: String,
    pub previous_uuid: Option<String>,
    pub phase: String,
}
fn valid_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
fn uuid() -> Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| Error::Platform("OS random source unavailable".into()))?;
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    let h: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    ))
}
fn id(uuid: &str) -> String {
    format!("dandelion-hotspot-{uuid}")
}

struct OwnedJournal<G: WriteGuard> {
    directory: PathBuf,
    _lock: File,
    guard: G,
}
impl<G: WriteGuard> OwnedJournal<G> {
    fn open(directory: &Path, guard: G) -> Result<Self> {
        if !directory.exists() {
            let parent = directory
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            guard.check(parent, 1024 * 1024)?;
            fs::DirBuilder::new().mode(0o700).create(directory)?;
        }
        let m = fs::symlink_metadata(directory)?;
        if !m.is_dir() || m.file_type().is_symlink() || m.permissions().mode() & 0o077 != 0 {
            return Err(Error::Config(
                "hotspot journal directory must be private (0700)".into(),
            ));
        }
        guard.check(directory, 1024 * 1024)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(directory.join("writer.lock"))?;
        lock.try_lock()
            .map_err(|_| Error::Platform("another hotspot operation owns the journal".into()))?;
        Ok(Self {
            directory: fs::canonicalize(directory)?,
            _lock: lock,
            guard,
        })
    }
    fn save(&self, journal: &Journal) -> Result<()> {
        self.guard.check(&self.directory, 1024 * 1024)?;
        let mut temp = tempfile::NamedTempFile::new_in(&self.directory)?;
        serde_json::to_writer(&mut temp, journal).map_err(|e| Error::State(e.to_string()))?;
        temp.as_file().sync_all()?;
        temp.persist(self.directory.join("hotspot.json"))
            .map_err(|e| Error::Io(e.error))?;
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }
}
pub fn inspect(directory: &Path) -> Result<Option<Journal>> {
    let path = directory.join("hotspot.json");
    if !path.exists() {
        return Ok(None);
    }
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > 65536
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(Error::State("unsafe hotspot journal".into()));
    }
    let mut bytes = Vec::new();
    File::open(path)?.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err(Error::State("hotspot journal too large".into()));
    }
    let journal: Journal = serde_json::from_slice(&bytes)
        .map_err(|_| Error::State("invalid hotspot journal".into()))?;
    journal.settings.validate()?;
    if journal.schema_version != 1
        || !valid_uuid(&journal.uuid)
        || journal
            .previous_uuid
            .as_deref()
            .is_some_and(|s| !valid_uuid(s))
    {
        return Err(Error::State("invalid hotspot ownership metadata".into()));
    }
    Ok(Some(journal))
}
async fn nm<P: ProcessRunner>(process: &P, args: Vec<String>) -> Result<String> {
    let output = process
        .run(Request {
            program: Program::NetworkManager,
            arguments: args.into_iter().map(Into::into).collect(),
            timeout: Duration::from_secs(20),
        })
        .await?;
    if output.code != 0 {
        return Err(Error::Platform(format!(
            "NetworkManager operation failed (exit {}); no privilege escalation attempted",
            output.code
        )));
    }
    String::from_utf8(output.stdout)
        .map(|s| s.trim().to_owned())
        .map_err(|_| Error::Platform("invalid NetworkManager response encoding".into()))
}
async fn active<P: ProcessRunner>(process: &P, interface: &str) -> Result<Option<String>> {
    let value = nm(
        process,
        vec![
            "-g".into(),
            "GENERAL.CON-UUID".into(),
            "device".into(),
            "show".into(),
            interface.into(),
        ],
    )
    .await?;
    if value.is_empty() || value == "--" {
        Ok(None)
    } else if valid_uuid(&value) {
        Ok(Some(value))
    } else {
        Err(Error::Platform("invalid active connection identity".into()))
    }
}
async fn rollback<P: ProcessRunner>(process: &P, journal: &Journal) -> Result<()> {
    let profiles = nm(
        process,
        vec![
            "-g".into(),
            "UUID".into(),
            "connection".into(),
            "show".into(),
        ],
    )
    .await?;
    if profiles.lines().any(|s| s == journal.uuid) {
        let profile_id = nm(
            process,
            vec![
                "-g".into(),
                "connection.id".into(),
                "connection".into(),
                "show".into(),
                "uuid".into(),
                journal.uuid.clone(),
            ],
        )
        .await?;
        if profile_id != id(&journal.uuid) {
            return Err(Error::Platform(
                "profile is not owned by this journal; refusing mutation".into(),
            ));
        }
        let current = active(process, &journal.settings.interface).await?;
        let restore = current.is_none() || current.as_deref() == Some(&journal.uuid);
        if current.as_deref() == Some(&journal.uuid) {
            nm(
                process,
                vec![
                    "--wait".into(),
                    "15".into(),
                    "connection".into(),
                    "down".into(),
                    "uuid".into(),
                    journal.uuid.clone(),
                ],
            )
            .await?;
        }
        nm(
            process,
            vec![
                "--wait".into(),
                "15".into(),
                "connection".into(),
                "delete".into(),
                "uuid".into(),
                journal.uuid.clone(),
            ],
        )
        .await?;
        if restore && let Some(previous) = &journal.previous_uuid {
            nm(
                process,
                vec![
                    "--wait".into(),
                    "15".into(),
                    "connection".into(),
                    "up".into(),
                    "uuid".into(),
                    previous.clone(),
                    "ifname".into(),
                    journal.settings.interface.clone(),
                ],
            )
            .await?;
        }
    }
    Ok(())
}

pub async fn apply<P: ProcessRunner, G: WriteGuard>(
    process: &P,
    guard: G,
    directory: &Path,
    settings: Settings,
    password: &str,
    cancel: &CancellationToken,
) -> Result<Journal> {
    settings.validate()?;
    if !(8..=63).contains(&password.len()) || !password.bytes().all(|b| (32..=126).contains(&b)) {
        return Err(Error::Config(
            "hotspot password must be 8..63 printable ASCII bytes in a private file".into(),
        ));
    }
    let store = OwnedJournal::open(directory, guard)?;
    if let Some(journal) = inspect(&store.directory)?
        && journal.phase != "stopped"
    {
        return Err(Error::State(
            "recover/stop the existing hotspot journal before applying another".into(),
        ));
    }
    let capability = nm(
        process,
        vec![
            "-g".into(),
            "WIFI-PROPERTIES.AP".into(),
            "device".into(),
            "show".into(),
            settings.interface.clone(),
        ],
    )
    .await?;
    if capability != "yes" {
        return Err(Error::Platform(
            "interface does not advertise AP support".into(),
        ));
    }
    let mut journal = Journal {
        schema_version: 1,
        previous_uuid: active(process, &settings.interface).await?,
        uuid: uuid()?,
        settings,
        phase: "creating".into(),
    };
    store.save(&journal)?;
    let mut secret = tempfile::NamedTempFile::new_in(&store.directory)?;
    writeln!(secret, "802-11-wireless-security.psk:{password}")?;
    secret.as_file().sync_all()?;
    let secret_path = secret
        .path()
        .to_str()
        .ok_or_else(|| Error::Config("password-file path cannot be represented".into()))?
        .to_owned();
    let operation = async {
        let args = vec![
            "--wait".into(),
            "15".into(),
            "connection".into(),
            "add".into(),
            "save".into(),
            "no".into(),
            "type".into(),
            "wifi".into(),
            "ifname".into(),
            journal.settings.interface.clone(),
            "con-name".into(),
            id(&journal.uuid),
            "ssid".into(),
            journal.settings.ssid.clone(),
            "connection.uuid".into(),
            journal.uuid.clone(),
            "connection.autoconnect".into(),
            "no".into(),
            "802-11-wireless.mode".into(),
            "ap".into(),
            "802-11-wireless.band".into(),
            journal.settings.band.clone(),
            "802-11-wireless-security.key-mgmt".into(),
            "wpa-psk".into(),
            "802-11-wireless-security.psk-flags".into(),
            "2".into(),
            "ipv4.method".into(),
            "shared".into(),
            "ipv6.method".into(),
            "disabled".into(),
        ];
        nm(process, args).await?;
        nm(
            process,
            vec![
                "--wait".into(),
                "15".into(),
                "connection".into(),
                "up".into(),
                "uuid".into(),
                journal.uuid.clone(),
                "ifname".into(),
                journal.settings.interface.clone(),
                "passwd-file".into(),
                secret_path,
            ],
        )
        .await?;
        Ok::<(), Error>(())
    };
    let result = tokio::select! {biased;_=cancel.cancelled()=>Err(Error::Cancelled),result=operation=>result};
    drop(secret);
    match result {
        Ok(()) => {
            journal.phase = "active".into();
            store.save(&journal)?;
            Ok(journal)
        }
        Err(error) => {
            let restored = rollback(process, &journal).await;
            journal.phase = if restored.is_ok() {
                "stopped"
            } else {
                "rollback-required"
            }
            .into();
            store.save(&journal)?;
            restored?;
            Err(error)
        }
    }
}
pub async fn stop<P: ProcessRunner, G: WriteGuard>(
    process: &P,
    guard: G,
    directory: &Path,
) -> Result<Journal> {
    let store = OwnedJournal::open(directory, guard)?;
    let mut journal = inspect(&store.directory)?
        .ok_or_else(|| Error::State("no hotspot ownership journal".into()))?;
    rollback(process, &journal).await?;
    journal.phase = "stopped".into();
    store.save(&journal)?;
    Ok(journal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dandelion_core::process::Output;
    use std::sync::Mutex;
    struct Guard;
    impl WriteGuard for Guard {
        fn check(&self, _: &Path, _: u64) -> Result<()> {
            Ok(())
        }
    }
    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<Vec<String>>>,
        profile: Mutex<Option<String>>,
        active: Mutex<bool>,
        fail_activate: bool,
    }
    impl ProcessRunner for Fake {
        async fn run(&self, r: Request) -> Result<Output> {
            let args: Vec<_> = r
                .arguments
                .iter()
                .map(|s| s.to_string_lossy().into_owned())
                .collect();
            self.calls.lock().unwrap().push(args.clone());
            let mut out = String::new();
            if args.get(1).map(String::as_str) == Some("WIFI-PROPERTIES.AP") {
                out = "yes".into();
            } else if args.get(1).map(String::as_str) == Some("GENERAL.CON-UUID") {
                if *self.active.lock().unwrap() {
                    out = self.profile.lock().unwrap().clone().unwrap();
                }
            } else if args.get(1).map(String::as_str) == Some("UUID") {
                out = self.profile.lock().unwrap().clone().unwrap_or_default();
            } else if args.get(1).map(String::as_str) == Some("connection.id") {
                out = id(self.profile.lock().unwrap().as_deref().unwrap());
            } else if args.iter().any(|s| s == "add") {
                let i = args.iter().position(|s| s == "connection.uuid").unwrap();
                *self.profile.lock().unwrap() = Some(args[i + 1].clone());
            } else if args.iter().any(|s| s == "up") {
                if self.fail_activate {
                    return Ok(Output {
                        code: 1,
                        stdout: Vec::new(),
                    });
                }
                *self.active.lock().unwrap() = true;
            } else if args.iter().any(|s| s == "down") {
                *self.active.lock().unwrap() = false;
            } else if args.iter().any(|s| s == "delete") {
                *self.profile.lock().unwrap() = None;
            }
            Ok(Output {
                code: 0,
                stdout: out.into_bytes(),
            })
        }
    }
    #[tokio::test]
    async fn explicit_lifecycle_never_passes_password_in_arguments() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("journal");
        let fake = Fake::default();
        let settings = Settings {
            interface: "wlan0".into(),
            ssid: "Local-Test".into(),
            band: "a".into(),
        };
        let j = apply(
            &fake,
            Guard,
            &path,
            settings,
            "test-only-password",
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(j.phase, "active");
        let j = stop(&fake, Guard, &path).await.unwrap();
        assert_eq!(j.phase, "stopped");
        assert!(!format!("{:?}", fake.calls.lock().unwrap()).contains("test-only-password"));
        assert!(
            fs::read_dir(path).unwrap().all(|e| !e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".tmp"))
        );
    }
    #[test]
    fn plan_rejects_injection_and_invalid_interfaces() {
        assert!(
            plan(Settings {
                interface: "wlan0;false".into(),
                ssid: "ok".into(),
                band: "a".into()
            })
            .is_err()
        );
        let p = plan(Settings {
            interface: "wlan0".into(),
            ssid: "ok".into(),
            band: "a".into(),
        })
        .unwrap();
        assert!(!p.password_on_argv);
        assert!(!p.regulatory_changes);
    }

    #[tokio::test]
    async fn failed_activation_removes_only_the_owned_profile() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("journal");
        let fake = Fake {
            fail_activate: true,
            ..Default::default()
        };
        let settings = Settings {
            interface: "wlan0".into(),
            ssid: "Test".into(),
            band: "a".into(),
        };
        assert!(
            apply(
                &fake,
                Guard,
                &path,
                settings,
                "test-only-password",
                &CancellationToken::new()
            )
            .await
            .is_err()
        );
        assert!(fake.profile.lock().unwrap().is_none());
        assert_eq!(inspect(&path).unwrap().unwrap().phase, "stopped");
    }
}
