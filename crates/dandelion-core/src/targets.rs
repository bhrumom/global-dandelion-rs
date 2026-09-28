//! Exact-destination capability policy. Discovery never constructs this token.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    net::{IpAddr, SocketAddr},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
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
pub fn authorize(
    peers: &[Peer],
    wan_enabled: bool,
    wan_allowlist: &[SocketAddr],
) -> Result<Vec<AuthorizedPeer>> {
    if peers.is_empty() || peers.len() > 8 || wan_allowlist.len() > 64 {
        return Err(Error::Config(
            "TLS needs 1..8 explicit peers and at most 64 WAN allowlist entries".into(),
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
                if !wan_enabled || !wan_allowlist.contains(&peer.address) {
                    return Err(Error::Config(
                        "WAN requires enable plus exact address allowlist".into(),
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
        for a in [
            "0.0.0.0:80",
            "255.255.255.255:80",
            "224.0.0.1:80",
            "[::ffff:127.0.0.1]:80",
            "127.0.0.1:0",
        ] {
            assert!(validate_unicast(a.parse().unwrap()).is_err());
        }
        let mut p = peer("127.0.0.1:2000", NetworkClass::Private);
        p.authorized = false;
        assert!(authorize(&[p], false, &[]).is_err());
    }
}
