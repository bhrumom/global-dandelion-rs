//! Exact-destination capability policy. Discovery never constructs this token.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
};

const MAX_TARGETS_PER_SESSION: usize = 8;
const MAX_POOL_ENTRIES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkClass {
    Lan,
    Private,
    Wan,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Peer {
    pub address: SocketAddr,
    pub server_name: String,
    pub class: NetworkClass,
    pub authorized: bool,
}

#[derive(Clone, Debug)]
pub struct AuthorizedPeer {
    address: SocketAddr,
    server_name: String,
}
impl AuthorizedPeer {
    pub fn address(&self) -> SocketAddr {
        self.address
    }
    pub fn server_name(&self) -> &str {
        &self.server_name
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetSelectionMode {
    #[default]
    Sequential,
    Randomized,
    DeterministicSeeded,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetPoolEntry {
    pub cidr: String,
    pub port: u16,
    pub server_name: String,
}

#[derive(Clone, Debug)]
struct TargetRange {
    start: u128,
    end: u128,
    ipv6: bool,
    port: u16,
    server_name: String,
    slots: u64,
}

#[derive(Clone, Debug)]
pub struct TargetPool {
    ranges: Vec<TargetRange>,
    slots: u64,
}

fn private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_private() || ip.is_loopback(),
        IpAddr::V6(ip) => ip.is_unique_local() || ip.is_loopback(),
    }
}

pub fn validate_unicast(address: SocketAddr) -> Result<()> {
    let invalid = address.port() == 0
        || address.ip().is_unspecified()
        || address.ip().is_multicast()
        || match address.ip() {
            IpAddr::V4(ip) => {
                ip.is_broadcast()
                    || ip.is_link_local()
                    || ip.octets()[0] == 0
                    || ip.octets()[0] >= 240
            }
            IpAddr::V6(ip) => ip.to_ipv4_mapped().is_some() || ip.is_unicast_link_local(),
        };
    if invalid {
        return Err(Error::Config(format!(
            "unsupported unicast destination {address}"
        )));
    }
    Ok(())
}

fn overlap(start: u128, end: u128, blocked_start: u128, blocked_end: u128) -> bool {
    start <= blocked_end && blocked_start <= end
}

fn v4(value: [u8; 4]) -> u128 {
    u128::from(u32::from_be_bytes(value))
}

fn public_range(start: u128, end: u128, ipv6: bool) -> bool {
    if ipv6 {
        let unique_local_start = 0xfc00_u128 << 112;
        let unique_local_end = (0xfdff_u128 << 112) | ((1_u128 << 112) - 1);
        let link_local_start = 0xfe80_u128 << 112;
        let link_local_end = (0xfebf_u128 << 112) | ((1_u128 << 112) - 1);
        let multicast_start = 0xff00_u128 << 112;
        let mapped_start = 0xffff_u128 << 32;
        let mapped_end = mapped_start | u128::from(u32::MAX);
        !overlap(start, end, 0, 1)
            && !overlap(start, end, unique_local_start, unique_local_end)
            && !overlap(start, end, link_local_start, link_local_end)
            && !overlap(start, end, multicast_start, u128::MAX)
            && !overlap(start, end, mapped_start, mapped_end)
    } else {
        let blocked = [
            (v4([0, 0, 0, 0]), v4([0, 255, 255, 255])),
            (v4([10, 0, 0, 0]), v4([10, 255, 255, 255])),
            (v4([100, 64, 0, 0]), v4([100, 127, 255, 255])),
            (v4([127, 0, 0, 0]), v4([127, 255, 255, 255])),
            (v4([169, 254, 0, 0]), v4([169, 254, 255, 255])),
            (v4([172, 16, 0, 0]), v4([172, 31, 255, 255])),
            (v4([192, 168, 0, 0]), v4([192, 168, 255, 255])),
            (v4([198, 18, 0, 0]), v4([198, 19, 255, 255])),
            (v4([224, 0, 0, 0]), v4([255, 255, 255, 255])),
        ];
        blocked
            .into_iter()
            .all(|(blocked_start, blocked_end)| !overlap(start, end, blocked_start, blocked_end))
    }
}

fn parse_cidr(value: &str) -> Result<(u128, u128, bool)> {
    let (raw_ip, raw_prefix) = value
        .split_once('/')
        .ok_or_else(|| Error::Config(format!("target pool entry needs CIDR prefix: {value}")))?;
    if raw_prefix.contains('/') {
        return Err(Error::Config(format!("invalid target CIDR {value}")));
    }
    let ip: IpAddr = raw_ip
        .parse()
        .map_err(|_| Error::Config(format!("invalid target CIDR address {value}")))?;
    let prefix: u8 = raw_prefix
        .parse()
        .map_err(|_| Error::Config(format!("invalid target CIDR prefix {value}")))?;
    let (numeric, bits, ipv6) = match ip {
        IpAddr::V4(ip) => (u128::from(u32::from(ip)), 32_u8, false),
        IpAddr::V6(ip) => (u128::from(ip), 128_u8, true),
    };
    if prefix > bits {
        return Err(Error::Config(format!("invalid target CIDR prefix {value}")));
    }
    let host_bits = bits - prefix;
    let host_mask = match host_bits {
        0 => 0,
        128 => u128::MAX,
        n => (1_u128 << n) - 1,
    };
    let start = numeric & !host_mask;
    if start != numeric {
        return Err(Error::Config(format!(
            "target CIDR must use its canonical network address: {value}"
        )));
    }
    let end = start | host_mask;
    if !public_range(start, end, ipv6) {
        return Err(Error::Config(format!(
            "target CIDR must contain only public unicast addresses: {value}"
        )));
    }
    let first = if ipv6 {
        IpAddr::V6(Ipv6Addr::from(start))
    } else {
        IpAddr::V4(Ipv4Addr::from(
            u32::try_from(start).map_err(|_| Error::Config("invalid IPv4 range".into()))?,
        ))
    };
    let last = if ipv6 {
        IpAddr::V6(Ipv6Addr::from(end))
    } else {
        IpAddr::V4(Ipv4Addr::from(
            u32::try_from(end).map_err(|_| Error::Config("invalid IPv4 range".into()))?,
        ))
    };
    if private(first) || private(last) {
        return Err(Error::Config(format!(
            "target CIDR must not include private or loopback addresses: {value}"
        )));
    }
    Ok((start, end, ipv6))
}

impl TargetPool {
    pub fn new(entries: &[TargetPoolEntry]) -> Result<Self> {
        if entries.is_empty() || entries.len() > MAX_POOL_ENTRIES {
            return Err(Error::Config(format!(
                "target pool needs 1..{MAX_POOL_ENTRIES} CIDR entries"
            )));
        }
        let mut ranges = Vec::with_capacity(entries.len());
        let mut slots = 0_u64;
        for entry in entries {
            if entry.port == 0
                || entry.server_name.is_empty()
                || entry.server_name.len() > 253
                || !entry.server_name.is_ascii()
            {
                return Err(Error::Config(
                    "target pool entries need a nonzero port and bounded ASCII TLS server name"
                        .into(),
                ));
            }
            let (start, end, ipv6) = parse_cidr(&entry.cidr)?;
            let slot_count = end
                .checked_sub(start)
                .and_then(|value| value.checked_add(1))
                .and_then(|value| u64::try_from(value).ok())
                .ok_or_else(|| {
                    Error::Config(format!(
                        "target CIDR {} is too large for the bounded selector",
                        entry.cidr
                    ))
                })?;
            for existing in &ranges {
                if existing.ipv6 == ipv6
                    && existing.port == entry.port
                    && overlap(start, end, existing.start, existing.end)
                {
                    return Err(Error::Config(
                        "target pool entries with the same port must not overlap".into(),
                    ));
                }
            }
            slots = slots
                .checked_add(slot_count)
                .ok_or_else(|| Error::Config("target pool cardinality overflow".into()))?;
            ranges.push(TargetRange {
                start,
                end,
                ipv6,
                port: entry.port,
                server_name: entry.server_name.clone(),
                slots: slot_count,
            });
        }
        Ok(Self { ranges, slots })
    }

    pub fn len(&self) -> u64 {
        self.slots
    }

    pub fn is_empty(&self) -> bool {
        self.slots == 0
    }

    pub fn contains(&self, address: SocketAddr) -> bool {
        let (numeric, ipv6) = match address.ip() {
            IpAddr::V4(ip) => (u128::from(u32::from(ip)), false),
            IpAddr::V6(ip) => (u128::from(ip), true),
        };
        self.ranges.iter().any(|range| {
            range.ipv6 == ipv6
                && range.port == address.port()
                && numeric >= range.start
                && numeric <= range.end
        })
    }

    fn peer_at(&self, mut index: u64) -> Result<Peer> {
        if index >= self.slots {
            return Err(Error::Config("target selector index out of range".into()));
        }
        for range in &self.ranges {
            if index >= range.slots {
                index -= range.slots;
                continue;
            }
            let numeric = range
                .start
                .checked_add(u128::from(index))
                .ok_or_else(|| Error::Config("target address overflow".into()))?;
            let ip = if range.ipv6 {
                IpAddr::V6(Ipv6Addr::from(numeric))
            } else {
                IpAddr::V4(Ipv4Addr::from(
                    u32::try_from(numeric)
                        .map_err(|_| Error::Config("invalid IPv4 target".into()))?,
                ))
            };
            let address = SocketAddr::new(ip, range.port);
            validate_unicast(address)?;
            if private(ip) || !self.contains(address) {
                return Err(Error::Config(
                    "target selector produced an address outside authorized public pool".into(),
                ));
            }
            return Ok(Peer {
                address,
                server_name: range.server_name.clone(),
                class: NetworkClass::Wan,
                authorized: true,
            });
        }
        Err(Error::Config("target selector index out of range".into()))
    }
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

#[derive(Debug)]
pub struct TargetSelector<'a> {
    pool: &'a TargetPool,
    cursor: u64,
    step: u64,
    emitted: u64,
}

impl<'a> TargetSelector<'a> {
    pub fn new(
        pool: &'a TargetPool,
        mode: TargetSelectionMode,
        seed: Option<u64>,
    ) -> Result<Self> {
        if pool.is_empty() {
            return Err(Error::Config("target pool is empty".into()));
        }
        let (cursor, step) = match mode {
            TargetSelectionMode::Sequential => {
                if seed.is_some() {
                    return Err(Error::Config(
                        "sequential target selection does not accept a seed".into(),
                    ));
                }
                (0, 1)
            }
            TargetSelectionMode::Randomized | TargetSelectionMode::DeterministicSeeded => {
                let mut state = seed.ok_or_else(|| {
                    Error::Config("randomized target selection requires a seed".into())
                })?;
                let cursor = splitmix64(&mut state) % pool.len();
                let mut step = if pool.len() == 1 {
                    1
                } else {
                    (splitmix64(&mut state) % (pool.len() - 1)) + 1
                };
                if pool.len() > 1 && gcd(step, pool.len()) != 1 {
                    for _ in 0..64 {
                        step = (step % (pool.len() - 1)) + 1;
                        if gcd(step, pool.len()) == 1 {
                            break;
                        }
                    }
                    if gcd(step, pool.len()) != 1 {
                        step = 1;
                    }
                }
                (cursor, step)
            }
        };
        Ok(Self {
            pool,
            cursor,
            step,
            emitted: 0,
        })
    }

    pub fn select(&mut self, count: usize) -> Result<Vec<Peer>> {
        if count == 0 || count > MAX_TARGETS_PER_SESSION {
            return Err(Error::Config(format!(
                "target selection count must be 1..{MAX_TARGETS_PER_SESSION}"
            )));
        }
        let count_u64 =
            u64::try_from(count).map_err(|_| Error::Config("target selection count overflow".into()))?;
        if self
            .emitted
            .checked_add(count_u64)
            .is_none_or(|total| total > self.pool.len())
        {
            return Err(Error::Config(
                "target selection exceeds unique authorized pool cardinality".into(),
            ));
        }
        let mut selected = Vec::with_capacity(count);
        for _ in 0..count {
            selected.push(self.pool.peer_at(self.cursor)?);
            self.emitted += 1;
            if self.pool.len() > 1 {
                self.cursor = ((u128::from(self.cursor) + u128::from(self.step))
                    % u128::from(self.pool.len())) as u64;
            }
        }
        Ok(selected)
    }
}

pub fn authorize(
    peers: &[Peer],
    wan_enabled: bool,
    wan_allowlist: &[SocketAddr],
) -> Result<Vec<AuthorizedPeer>> {
    authorize_with_pool(peers, wan_enabled, wan_allowlist, None)
}

pub fn authorize_with_pool(
    peers: &[Peer],
    wan_enabled: bool,
    wan_allowlist: &[SocketAddr],
    target_pool: Option<&TargetPool>,
) -> Result<Vec<AuthorizedPeer>> {
    if peers.is_empty() || peers.len() > MAX_TARGETS_PER_SESSION || wan_allowlist.len() > 64 {
        return Err(Error::Config(
            "TLS needs 1..8 explicit/selected peers and at most 64 WAN allowlist entries".into(),
        ));
    }
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for peer in peers {
        validate_unicast(peer.address)?;
        if !peer.authorized
            || !seen.insert(peer.address)
            || peer.server_name.is_empty()
            || peer.server_name.len() > 253
            || !peer.server_name.is_ascii()
        {
            return Err(Error::Config(
                "every peer needs explicit authorization, unique address and a bounded TLS name"
                    .into(),
            ));
        }
        match peer.class {
            NetworkClass::Wan => {
                let in_pool = target_pool.is_some_and(|pool| pool.contains(peer.address));
                if !wan_enabled || (!wan_allowlist.contains(&peer.address) && !in_pool) {
                    return Err(Error::Config(
                        "WAN requires enable plus exact allowlist or authorized target pool".into(),
                    ));
                }
            }
            _ => {
                if !private(peer.address.ip()) {
                    return Err(Error::Config(
                        "LAN/private peer must use private or loopback addressing".into(),
                    ));
                }
            }
        }
        result.push(AuthorizedPeer {
            address: peer.address,
            server_name: peer.server_name.clone(),
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(address: &str, class: NetworkClass) -> Peer {
        Peer {
            address: address.parse().unwrap(),
            class,
            server_name: "localhost".into(),
            authorized: true,
        }
    }

    fn pool_entry(cidr: &str) -> TargetPoolEntry {
        TargetPoolEntry {
            cidr: cidr.into(),
            port: 443,
            server_name: "example.invalid".into(),
        }
    }

    #[test]
    fn wan_requires_both_explicit_controls() {
        let p = peer("1.1.1.1:443", NetworkClass::Wan);
        assert!(authorize(std::slice::from_ref(&p), false, &[p.address]).is_err());
        assert!(authorize(std::slice::from_ref(&p), true, &[]).is_err());
        assert!(authorize(std::slice::from_ref(&p), true, &[p.address]).is_ok());
        assert!(authorize(&[peer("1.1.1.1:443", NetworkClass::Private)], false, &[]).is_err());
    }

    #[test]
    fn reject_implicit_and_invalid_targets() {
        for address in [
            "0.0.0.0:80",
            "255.255.255.255:80",
            "224.0.0.1:80",
            "[::ffff:127.0.0.1]:80",
            "127.0.0.1:0",
        ] {
            assert!(validate_unicast(address.parse().unwrap()).is_err());
        }
        let mut p = peer("127.0.0.1:2000", NetworkClass::Private);
        p.authorized = false;
        assert!(authorize(&[p], false, &[]).is_err());
    }

    #[test]
    fn pool_selection_is_bounded_reproducible_and_revalidated() {
        let pool = TargetPool::new(&[pool_entry("8.8.8.0/29")]).unwrap();
        assert_eq!(pool.len(), 8);

        let mut sequential =
            TargetSelector::new(&pool, TargetSelectionMode::Sequential, None).unwrap();
        let selected = sequential.select(3).unwrap();
        assert_eq!(selected[0].address, "8.8.8.0:443".parse().unwrap());
        assert_eq!(selected[1].address, "8.8.8.1:443".parse().unwrap());
        assert_eq!(selected[2].address, "8.8.8.2:443".parse().unwrap());

        let mut first =
            TargetSelector::new(&pool, TargetSelectionMode::Randomized, Some(42)).unwrap();
        let mut replay =
            TargetSelector::new(&pool, TargetSelectionMode::Randomized, Some(42)).unwrap();
        let first = first.select(4).unwrap();
        let replay = replay.select(4).unwrap();
        assert_eq!(
            first.iter().map(|peer| peer.address).collect::<Vec<_>>(),
            replay.iter().map(|peer| peer.address).collect::<Vec<_>>()
        );
        assert!(authorize(&first, true, &[]).is_err());
        assert!(authorize_with_pool(&first, true, &[], Some(&pool)).is_ok());
    }

    #[test]
    fn pool_rejects_scope_widening_and_duplicate_ranges() {
        assert!(TargetPool::new(&[pool_entry("10.0.0.0/24")]).is_err());
        assert!(TargetPool::new(&[pool_entry("169.254.0.0/16")]).is_err());
        assert!(TargetPool::new(&[pool_entry("8.8.8.1/29")]).is_err());
        assert!(
            TargetPool::new(&[pool_entry("8.8.8.0/29"), pool_entry("8.8.8.4/30")]).is_err()
        );
        let pool = TargetPool::new(&[pool_entry("8.8.8.8/32")]).unwrap();
        let mut selector =
            TargetSelector::new(&pool, TargetSelectionMode::DeterministicSeeded, Some(7)).unwrap();
        assert!(selector.select(2).is_err());
    }
}
