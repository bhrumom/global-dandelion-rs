//! Deterministic defaults < TOML < environment < explicit CLI overrides.
use dandelion_core::{Error, HEADER_BYTES, MAX_DATAGRAM_BYTES, MAX_PAYLOAD_BYTES, Result};
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub runtime: Runtime,
    pub scanner: Scanner,
    pub pacing: Pacing,
    pub transport: Transport,
    pub state: State,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Runtime {
    pub queue_capacity: usize,
    pub max_pipeline_bytes: usize,
    pub shutdown_timeout_ms: u64,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            queue_capacity: 32,
            max_pipeline_bytes: 8 * 1024 * 1024,
            shutdown_timeout_ms: 3000,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Scanner {
    pub roots: Vec<PathBuf>,
    pub chunk_bytes: usize,
    pub exclude_hidden: bool,
    pub excluded_names: Vec<String>,
    pub max_depth: usize,
    pub max_files: u64,
    pub max_read_bytes: u64,
    pub allowed_mount_prefixes: Vec<PathBuf>,
}
impl Default for Scanner {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            chunk_bytes: MAX_PAYLOAD_BYTES,
            exclude_hidden: true,
            excluded_names: vec![
                "target".into(),
                "node_modules".into(),
                "__pycache__".into(),
                "dandelion.toml".into(),
                "dandelion-state.json".into(),
            ],
            max_depth: 64,
            max_files: 1_000_000,
            max_read_bytes: 256 * 1024 * 1024,
            allowed_mount_prefixes: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Pacing {
    pub bytes_per_second: u64,
    pub burst_bytes: u64,
    pub traffic_limit_bytes: u64,
    pub schedule: Option<dandelion_core::schedule::DayNight>,
}
impl Default for Pacing {
    fn default() -> Self {
        Self {
            bytes_per_second: 1024 * 1024,
            burst_bytes: MAX_DATAGRAM_BYTES as u64,
            traffic_limit_bytes: 64 * 1024 * 1024,
            schedule: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Transport {
    pub mode: Mode,
    pub tls: Tls,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tls {
    pub ca_file: PathBuf,
    pub certificate_file: PathBuf,
    pub private_key_file: PathBuf,
    pub peers: Vec<dandelion_core::targets::Peer>,
    pub wan_enabled: bool,
    pub wan_allowlist: Vec<std::net::SocketAddr>,
    pub timeout_ms: u64,
    pub connect_attempts: u8,
}
impl Default for Tls {
    fn default() -> Self {
        Self {
            ca_file: PathBuf::new(),
            certificate_file: PathBuf::new(),
            private_key_file: PathBuf::new(),
            peers: Vec::new(),
            wan_enabled: false,
            wan_allowlist: Vec::new(),
            timeout_ms: 10000,
            connect_attempts: 1,
        }
    }
}
impl Tls {
    pub fn validate_credentials(&self) -> Result<()> {
        if [
            &self.ca_file,
            &self.certificate_file,
            &self.private_key_file,
        ]
        .iter()
        .any(|p| p.as_os_str().is_empty())
            || !(100..=60000).contains(&self.timeout_ms)
            || !(1..=3).contains(&self.connect_attempts)
        {
            return Err(Error::Config("TLS requires CA/certificate/key file paths, timeout 100..60000 ms and 1..3 connect attempts".into()));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Loopback,
    Tls,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct State {
    pub enabled: bool,
    pub directory: PathBuf,
    pub lifetime_limit_bytes: u64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            enabled: false,
            directory: PathBuf::from(".dandelion-state"),
            lifetime_limit_bytes: 180 * 1024 * 1024 * 1024,
        }
    }
}

/// Constructible only through validation. Runtime code cannot mutate the config.
#[derive(Clone, Debug)]
pub struct ValidatedConfig(Config);
impl ValidatedConfig {
    pub fn get(&self) -> &Config {
        &self.0
    }
    pub fn require_sources(&self) -> Result<()> {
        if self.0.scanner.roots.is_empty() && self.0.scanner.allowed_mount_prefixes.is_empty() {
            return Err(Error::Config(
                "set scanner.roots or pass --source DIR".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct Overrides {
    pub roots: Option<Vec<PathBuf>>,
    pub queue_capacity: Option<usize>,
    pub chunk_bytes: Option<usize>,
    pub bytes_per_second: Option<u64>,
    pub traffic_limit_bytes: Option<u64>,
}

impl Config {
    pub fn validate(self) -> Result<ValidatedConfig> {
        let fail = |message: &str| Error::Config(message.into());
        if !(1..=4096).contains(&self.runtime.queue_capacity) {
            return Err(fail("runtime.queue_capacity must be 1..4096"));
        }
        if !(1..=MAX_PAYLOAD_BYTES).contains(&self.scanner.chunk_bytes) {
            return Err(fail(
                "scanner.chunk_bytes must be 1..1152 (MTU-safe packet v2)",
            ));
        }
        let bound = (self.runtime.queue_capacity + 3)
            .checked_mul(self.scanner.chunk_bytes + HEADER_BYTES)
            .ok_or_else(|| fail("pipeline memory calculation overflow"))?;
        if bound > self.runtime.max_pipeline_bytes
            || self.runtime.max_pipeline_bytes > 64 * 1024 * 1024
        {
            return Err(fail(
                "configured queue exceeds max_pipeline_bytes, or memory cap exceeds 64 MiB",
            ));
        }
        if !(100..=60_000).contains(&self.runtime.shutdown_timeout_ms) {
            return Err(fail("shutdown_timeout_ms must be 100..60000"));
        }
        if self.scanner.roots.len() > 64 || !(1..=64).contains(&self.scanner.max_depth) {
            return Err(fail("at most 64 roots; max_depth must be 1..64"));
        }
        if self.scanner.max_files == 0 || self.scanner.max_read_bytes == 0 {
            return Err(fail("scanner file/read budgets must be positive"));
        }
        if self.scanner.excluded_names.len() > 256
            || self
                .scanner
                .excluded_names
                .iter()
                .any(|s| s.len() > 255 || s.contains('/'))
        {
            return Err(fail(
                "excluded_names requires at most 256 single-component names",
            ));
        }
        if !(1..=1024 * 1024 * 1024).contains(&self.pacing.bytes_per_second) {
            return Err(fail("pacing.bytes_per_second must be 1..1073741824"));
        }
        if self.pacing.burst_bytes < (self.scanner.chunk_bytes + HEADER_BYTES) as u64
            || self.pacing.burst_bytes > 16 * 1024 * 1024
        {
            return Err(fail(
                "burst_bytes must hold a complete packet and be at most 16 MiB",
            ));
        }
        if self.pacing.traffic_limit_bytes == 0 {
            return Err(fail(
                "traffic_limit_bytes must be positive; unlimited sending is not supported",
            ));
        }
        if self.state.enabled
            && (self.state.directory.as_os_str().is_empty() || self.state.lifetime_limit_bytes == 0)
        {
            return Err(fail(
                "enabled state needs a directory and positive lifetime limit",
            ));
        }
        if let Some(schedule) = &self.pacing.schedule {
            schedule.validate()?;
        }
        if self.scanner.allowed_mount_prefixes.len() > 64
            || self.scanner.allowed_mount_prefixes.iter().any(|p| {
                !p.is_absolute()
                    || p.parent().is_none()
                    || p.components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))
            })
        {
            return Err(fail(
                "allowed_mount_prefixes must be absolute non-root paths without '..' (max 64)",
            ));
        }
        if matches!(self.transport.mode, Mode::Tls) {
            self.transport.tls.validate_credentials()?;
            dandelion_core::targets::authorize(
                &self.transport.tls.peers,
                self.transport.tls.wan_enabled,
                &self.transport.tls.wan_allowlist,
            )?;
        }
        Ok(ValidatedConfig(self))
    }

    pub fn load(
        path: Option<&Path>,
        environment: impl IntoIterator<Item = (OsString, OsString)>,
        overrides: Overrides,
    ) -> Result<ValidatedConfig> {
        let mut value =
            toml::Value::try_from(Self::default()).map_err(|e| Error::Config(e.to_string()))?;
        if let Some(path) = path {
            let file = std::fs::File::open(path)?;
            if file.metadata()?.len() > 1024 * 1024 {
                return Err(Error::Config("config file exceeds 1 MiB".into()));
            }
            use std::io::Read;
            let mut text = String::new();
            file.take(1024 * 1024 + 1).read_to_string(&mut text)?;
            if text.len() > 1024 * 1024 {
                return Err(Error::Config("config file exceeds 1 MiB".into()));
            }
            let patch: toml::Value =
                toml::from_str(&text).map_err(|e| Error::Config(e.to_string()))?;
            merge(&mut value, patch);
        }
        for (key, raw) in environment {
            let Some(key) = key.to_str().and_then(|s| s.strip_prefix("DANDELION__")) else {
                continue;
            };
            let parts: Vec<_> = key.split("__").map(str::to_ascii_lowercase).collect();
            if parts.len() != 2 || parts.iter().any(String::is_empty) {
                return Err(Error::Config(format!(
                    "invalid environment key DANDELION__{key}"
                )));
            }
            let raw = raw.to_str().ok_or_else(|| {
                Error::Config(format!("environment value for {key} must be TOML/UTF-8"))
            })?;
            let parsed: toml::Value = toml::from_str(&format!("value = {raw}"))
                .map_err(|_| Error::Config(format!("{key} must contain a TOML value")))?;
            let section = value
                .get_mut(&parts[0])
                .and_then(toml::Value::as_table_mut)
                .ok_or_else(|| {
                    Error::Config(format!("unknown environment section {}", parts[0]))
                })?;
            section.insert(parts[1].clone(), parsed["value"].clone());
        }
        let mut config: Self = value
            .try_into()
            .map_err(|e: toml::de::Error| Error::Config(e.to_string()))?;
        if let Some(v) = overrides.roots {
            config.scanner.roots = v;
        }
        if let Some(v) = overrides.queue_capacity {
            config.runtime.queue_capacity = v;
        }
        if let Some(v) = overrides.chunk_bytes {
            config.scanner.chunk_bytes = v;
        }
        if let Some(v) = overrides.bytes_per_second {
            config.pacing.bytes_per_second = v;
        }
        if let Some(v) = overrides.traffic_limit_bytes {
            config.pacing.traffic_limit_bytes = v;
        }
        config.validate()
    }

    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| Error::Config(e.to_string()))
    }
}

fn merge(base: &mut toml::Value, patch: toml::Value) {
    if let (Some(base), Some(patch)) = (base.as_table_mut(), patch.as_table()) {
        for (key, value) in patch {
            if let Some(existing) = base.get_mut(key) {
                merge(existing, value.clone());
            } else {
                base.insert(key.clone(), value.clone());
            }
        }
    } else {
        *base = patch;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_are_bounded_and_local_only() {
        let c = Config::default().validate().unwrap();
        assert!(matches!(c.get().transport.mode, Mode::Loopback));
        assert!(c.require_sources().is_err());
        let c: Config = toml::from_str(&c.get().to_toml().unwrap()).unwrap();
        c.validate().unwrap();
    }
    #[test]
    fn precedence_cli_env_file_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[runtime]\nqueue_capacity = 8\n[pacing]\nbytes_per_second = 4000\n",
        )
        .unwrap();
        let env = vec![(
            OsString::from("DANDELION__PACING__BYTES_PER_SECOND"),
            OsString::from("5000"),
        )];
        let cfg = Config::load(Some(&path), env.clone(), Overrides::default()).unwrap();
        assert_eq!(cfg.get().runtime.queue_capacity, 8);
        assert_eq!(cfg.get().pacing.bytes_per_second, 5000);
        let cfg = Config::load(
            Some(&path),
            env,
            Overrides {
                bytes_per_second: Some(6000),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(cfg.get().pacing.bytes_per_second, 6000);
    }
    #[test]
    fn bad_config_rejected_before_startup() {
        for text in [
            "oops=1",
            "[transport]\nmode='wan'",
            "[runtime]\nqueue_capacity=0",
            "[pacing]\nbytes_per_second=0",
            "[scanner]\nchunk_bytes=65536",
        ] {
            let parsed = toml::from_str::<Config>(text);
            assert!(
                parsed.map(|c| c.validate().is_err()).unwrap_or(true),
                "{text}"
            );
        }
        let env = [(
            OsString::from("DANDELION__RUNTIME__TYPO"),
            OsString::from("1"),
        )];
        assert!(Config::load(None, env, Overrides::default()).is_err());
        let mut config = Config::default();
        config.runtime.max_pipeline_bytes = 1;
        assert!(config.validate().is_err());
    }

    #[test]
    fn distributed_configuration_examples_validate() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        for file in ["loopback.toml", "tls-client.toml", "service.toml"] {
            let path = root.join(file);
            Config::load(Some(&path), std::iter::empty(), Overrides::default()).unwrap();
        }
    }
}
