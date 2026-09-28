//! Bounded read-only proc/sys inventory. Observations never authorize targets.
use dandelion_core::{Error, Result};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File},
    io::Read,
    net::Ipv4Addr,
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
const MAX_TABLE: u64 = 16 * 1024 * 1024;
const MAX_ROWS: usize = 4096;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Mount {
    pub id: u64,
    pub parent_id: u64,
    pub device: String,
    pub root: PathBuf,
    pub mountpoint: PathBuf,
    pub filesystem: String,
    pub source: PathBuf,
    pub options: String,
}
#[derive(Debug, Serialize)]
pub struct Interface {
    pub name: String,
    pub state: String,
    pub mtu: u64,
    pub address: String,
    pub received_bytes: u64,
    pub transmitted_bytes: u64,
}
#[derive(Debug, Serialize)]
pub struct Neighbour {
    pub address: Ipv4Addr,
    pub mac: String,
    pub interface: String,
    pub flags: String,
}
#[derive(Debug, Serialize)]
pub struct Observation {
    pub schema_version: u32,
    pub unix_seconds: u64,
    pub mounts: Vec<Mount>,
    pub interfaces: Vec<Interface>,
    pub established_tcp: u64,
    pub neighbours_ipv4: Vec<Neighbour>,
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Default)]
pub struct MountChanges {
    pub added: Vec<Mount>,
    pub removed: Vec<Mount>,
}

pub fn bounded_text(path: &Path, limit: u64) -> Result<String> {
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(Error::Platform(format!(
            "{} exceeds input limit",
            path.display()
        )));
    }
    String::from_utf8(bytes).map_err(|_| {
        Error::Platform(format!(
            "{} contains unsupported text encoding",
            path.display()
        ))
    })
}
fn number<T: std::str::FromStr>(s: &str) -> Result<T> {
    s.parse()
        .map_err(|_| Error::Platform("invalid numeric field".into()))
}
fn unescape(value: &str) -> Result<PathBuf> {
    let mut out = Vec::new();
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            if i + 3 >= bytes.len()
                || !bytes[i + 1..i + 4]
                    .iter()
                    .all(|b| (b'0'..=b'7').contains(b))
            {
                return Err(Error::Platform("invalid mount escape".into()));
            }
            let n = (bytes[i + 1] - b'0') as u16 * 64
                + (bytes[i + 2] - b'0') as u16 * 8
                + (bytes[i + 3] - b'0') as u16;
            if n == 0 || n > 255 {
                return Err(Error::Platform("invalid mount path byte".into()));
            }
            out.push(n as u8);
            i += 4;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Ok(PathBuf::from(OsString::from_vec(out)))
}
pub fn parse_mountinfo(text: &str) -> Result<Vec<Mount>> {
    if text.len() as u64 > MAX_TABLE {
        return Err(Error::Platform("mount table too large".into()));
    }
    let mut mounts = Vec::new();
    for line in text.lines() {
        if mounts.len() == MAX_ROWS {
            return Err(Error::Platform("too many mounts".into()));
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        let sep = fields
            .iter()
            .position(|s| *s == "-")
            .ok_or_else(|| Error::Platform("missing mountinfo separator".into()))?;
        if sep < 6 || fields.len() != sep + 4 {
            return Err(Error::Platform("malformed mountinfo row".into()));
        }
        mounts.push(Mount {
            id: number(fields[0])?,
            parent_id: number(fields[1])?,
            device: fields[2].into(),
            root: unescape(fields[3])?,
            mountpoint: unescape(fields[4])?,
            options: fields[5].into(),
            filesystem: fields[sep + 1].into(),
            source: unescape(fields[sep + 2])?,
        });
    }
    Ok(mounts)
}
pub fn mounts() -> Result<Vec<Mount>> {
    parse_mountinfo(&bounded_text(Path::new("/proc/self/mountinfo"), MAX_TABLE)?)
}
pub fn diff_mounts(before: &[Mount], after: &[Mount]) -> MountChanges {
    let key = |m: &Mount| {
        (
            m.id,
            m.device.clone(),
            m.mountpoint.clone(),
            m.source.clone(),
        )
    };
    let old: BTreeMap<_, _> = before.iter().map(|m| (key(m), m)).collect();
    let new: BTreeMap<_, _> = after.iter().map(|m| (key(m), m)).collect();
    MountChanges {
        added: new
            .iter()
            .filter(|(k, _)| !old.contains_key(*k))
            .map(|(_, m)| (*m).clone())
            .collect(),
        removed: old
            .iter()
            .filter(|(k, _)| !new.contains_key(*k))
            .map(|(_, m)| (*m).clone())
            .collect(),
    }
}
/// Discovery is constrained to administrator-selected mount namespaces. Nothing
/// in this result becomes a network destination, and system roots are rejected.
pub fn authorized_mount_roots(table: &[Mount], prefixes: &[PathBuf]) -> Result<Vec<PathBuf>> {
    if prefixes.len() > 64
        || prefixes
            .iter()
            .any(|p| !p.is_absolute() || p.parent().is_none())
    {
        return Err(Error::Config(
            "mount prefixes must be absolute non-root directories (max 64)".into(),
        ));
    }
    let mut roots: Vec<_> = table
        .iter()
        .filter(|m| {
            m.source.starts_with("/dev") && prefixes.iter().any(|p| m.mountpoint.starts_with(p))
        })
        .map(|m| m.mountpoint.clone())
        .collect();
    roots.sort();
    roots.dedup();
    if roots.len() > 64 {
        return Err(Error::Config("discovered roots exceed 64".into()));
    }
    Ok(roots)
}
pub fn parse_established(text: &str) -> Result<u64> {
    let mut count = 0;
    let mut rows = 0;
    for line in text.lines().skip(1) {
        rows += 1;
        if rows > 100_000 {
            return Err(Error::Platform("connection table exceeds cap".into()));
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 4 {
            return Err(Error::Platform("malformed TCP table".into()));
        }
        if fields[3] == "01" {
            count += 1;
        }
    }
    Ok(count)
}
pub fn parse_arp(text: &str) -> Result<Vec<Neighbour>> {
    let mut rows = Vec::new();
    for line in text.lines().skip(1) {
        if rows.len() == MAX_ROWS {
            return Err(Error::Platform("neighbour table exceeds cap".into()));
        }
        let f: Vec<_> = line.split_whitespace().collect();
        if f.len() != 6 {
            return Err(Error::Platform("malformed ARP row".into()));
        }
        rows.push(Neighbour {
            address: number(f[0])?,
            flags: f[2].into(),
            mac: f[3].into(),
            interface: f[5].into(),
        });
    }
    Ok(rows)
}
pub fn observe() -> Result<Observation> {
    let mut warnings = Vec::new();
    let mounts = mounts()?;
    let mut interfaces = Vec::new();
    for entry in fs::read_dir("/sys/class/net")? {
        if interfaces.len() == MAX_ROWS {
            return Err(Error::Platform("too many interfaces".into()));
        }
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| Error::Platform("non-UTF8 interface name".into()))?;
        let path = entry.path();
        let read = |file: &str| bounded_text(&path.join(file), 4096).map(|s| s.trim().to_owned());
        let one: Result<Interface> = (|| {
            Ok(Interface {
                name: name.clone(),
                state: read("operstate")?,
                mtu: number(&read("mtu")?)?,
                address: read("address")?,
                received_bytes: number(&read("statistics/rx_bytes")?)?,
                transmitted_bytes: number(&read("statistics/tx_bytes")?)?,
            })
        })();
        match one {
            Ok(i) => interfaces.push(i),
            Err(e) => warnings.push(format!("interface {name}: {e}")),
        }
    }
    interfaces.sort_by(|a, b| a.name.cmp(&b.name));
    let mut established_tcp = 0;
    for path in ["/proc/net/tcp", "/proc/net/tcp6"] {
        match bounded_text(Path::new(path), MAX_TABLE).and_then(|s| parse_established(&s)) {
            Ok(n) => established_tcp += n,
            Err(e) => warnings.push(e.to_string()),
        }
    }
    let neighbours_ipv4 =
        match bounded_text(Path::new("/proc/net/arp"), MAX_TABLE).and_then(|s| parse_arp(&s)) {
            Ok(rows) => rows,
            Err(e) => {
                warnings.push(e.to_string());
                Vec::new()
            }
        };
    Ok(Observation {
        schema_version: 1,
        unix_seconds: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| Error::Platform(e.to_string()))?
            .as_secs(),
        mounts,
        interfaces,
        established_tcp,
        neighbours_ipv4,
        warnings,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    const M: &str = "36 25 8:1 / /media/My\\040Disk rw,relatime shared:1 - ext4 /dev/sdb1 rw\n";
    #[test]
    fn parses_escapes_and_replacement_identity() {
        let a = parse_mountinfo(M).unwrap();
        assert_eq!(a[0].mountpoint, Path::new("/media/My Disk"));
        let b = parse_mountinfo(&M.replace("8:1", "8:2")).unwrap();
        let diff = diff_mounts(&a, &b);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.removed.len(), 1);
        assert!(parse_mountinfo("broken").is_err());
    }
    #[test]
    fn discovery_requires_explicit_nonroot_prefix() {
        let m = parse_mountinfo(M).unwrap();
        assert!(authorized_mount_roots(&m, &[]).unwrap().is_empty());
        assert_eq!(
            authorized_mount_roots(&m, &["/media".into()])
                .unwrap()
                .len(),
            1
        );
        assert!(authorized_mount_roots(&m, &["/".into()]).is_err());
    }
    #[test]
    fn connections_and_neighbours_are_passive_parsers() {
        assert_eq!(
            parse_established("header\n0: x y 01\n1: x y 0A\n").unwrap(),
            1
        );
        assert!(parse_established("header\nbad").is_err());
        let n = parse_arp("header\n192.168.0.2 0x1 0x2 aa:bb:cc:dd:ee:ff * eth0\n").unwrap();
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].interface, "eth0");
    }
}
