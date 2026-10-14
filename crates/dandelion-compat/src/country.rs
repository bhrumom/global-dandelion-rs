//! Bounded GeoLite-style CSV metadata lookup. Does not generate addresses,
//! authorize peers, download proprietary databases or send network traffic.
use dandelion_core::{Error, Result};
use serde::Serialize;
use std::{collections::BTreeMap, net::IpAddr};
const MAX_BYTES: usize = 64 * 1024 * 1024;
fn fail(message: &str) -> Error {
    Error::Config(format!("country database: {message}"))
}
#[derive(Debug, Serialize)]
pub struct CountryMatch {
    pub address: IpAddr,
    pub country: Option<String>,
    pub network: Option<String>,
    pub authorized: bool,
}
fn header(headers: &csv::StringRecord, name: &str) -> Result<usize> {
    headers
        .iter()
        .position(|h| h == name)
        .ok_or_else(|| fail("required CSV header missing"))
}
fn field(record: &csv::StringRecord, index: usize) -> Result<&str> {
    record.get(index).ok_or_else(|| fail("missing CSV field"))
}
fn contains(cidr: &str, address: IpAddr) -> Result<(bool, u8)> {
    let (ip, bits) = cidr.split_once('/').ok_or_else(|| fail("invalid CIDR"))?;
    let ip: IpAddr = ip.parse().map_err(|_| fail("invalid network IP"))?;
    let bits: u8 = bits.parse().map_err(|_| fail("invalid network prefix"))?;
    match (ip, address) {
        (IpAddr::V4(network), IpAddr::V4(address)) => {
            if bits > 32 {
                return Err(fail("IPv4 prefix exceeds 32"));
            }
            let mask = if bits == 0 {
                0
            } else {
                u32::MAX << (32 - bits)
            };
            let network = u32::from(network);
            if network & !mask != 0 {
                return Err(fail("non-canonical IPv4 network"));
            }
            Ok((u32::from(address) & mask == network, bits))
        }
        (IpAddr::V6(network), IpAddr::V6(address)) => {
            if bits > 128 {
                return Err(fail("IPv6 prefix exceeds 128"));
            }
            let mask = if bits == 0 {
                0
            } else {
                u128::MAX << (128 - bits)
            };
            let network = u128::from(network);
            if network & !mask != 0 {
                return Err(fail("non-canonical IPv6 network"));
            }
            Ok((u128::from(address) & mask == network, bits))
        }
        (IpAddr::V4(_), _) if bits <= 32 => Ok((false, bits)),
        (IpAddr::V6(_), _) if bits <= 128 => Ok((false, bits)),
        _ => Err(fail("network prefix exceeds address width")),
    }
}
pub fn lookup(locations: &[u8], blocks: &[u8], address: IpAddr) -> Result<CountryMatch> {
    if locations.len() > MAX_BYTES || blocks.len() > MAX_BYTES {
        return Err(fail("input exceeds 64 MiB"));
    }
    let mut reader = csv::ReaderBuilder::new().from_reader(locations);
    let headers = reader
        .headers()
        .map_err(|_| fail("location CSV header invalid"))?
        .clone();
    let id = header(&headers, "geoname_id")?;
    let country = header(&headers, "country_name")?;
    let mut names = BTreeMap::new();
    for (row, record) in reader.records().enumerate() {
        if row >= 65536 {
            return Err(fail("location row limit exceeded"));
        }
        let record = record.map_err(|_| fail("location CSV record invalid"))?;
        let id = field(&record, id)?;
        let name = field(&record, country)?;
        if id.len() > 32 || name.len() > 256 {
            return Err(fail("location field exceeds bound"));
        }
        if !name.is_empty()
            && let Some(old) = names.insert(id.to_owned(), name.to_owned())
            && old != name
        {
            return Err(fail("conflicting geoname id"));
        }
    }
    let mut reader = csv::ReaderBuilder::new().from_reader(blocks);
    let headers = reader
        .headers()
        .map_err(|_| fail("blocks CSV header invalid"))?
        .clone();
    let net = header(&headers, "network")?;
    let id = header(&headers, "geoname_id")?;
    let mut best: Option<(u8, String, String)> = None;
    for (row, record) in reader.records().enumerate() {
        if row >= 1_000_000 {
            return Err(fail("network row limit exceeded"));
        }
        let record = record.map_err(|_| fail("blocks CSV record invalid"))?;
        let network = field(&record, net)?;
        if network.len() > 64 {
            return Err(fail("network field exceeds bound"));
        }
        let (matches, prefix) = contains(network, address)?;
        if matches && let Some(country) = names.get(field(&record, id)?) {
            if let Some((prior, prior_country, _)) = &best
                && *prior == prefix
                && prior_country != country
            {
                return Err(fail("conflicting equally specific metadata"));
            }
            if best.as_ref().is_none_or(|(prior, _, _)| prefix > *prior) {
                best = Some((prefix, country.clone(), network.to_owned()));
            }
        }
    }
    Ok(CountryMatch {
        address,
        country: best.as_ref().map(|(_, country, _)| country.clone()),
        network: best.map(|(_, _, network)| network),
        authorized: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv_metadata_never_authorizes_targets() {
        let locations = b"geoname_id,country_name\n1,Country A\n2,Country B\n";
        let blocks = b"network,geoname_id\n10.0.0.0/8,1\n10.1.0.0/16,2\n";
        let result = lookup(locations, blocks, "10.1.2.3".parse().unwrap()).unwrap();
        assert_eq!(result.country.as_deref(), Some("Country B"));
        assert!(!result.authorized);
    }
    #[test]
    fn malformed_and_conflicting_data_fail() {
        assert!(
            lookup(
                b"bad\n",
                b"network,geoname_id\n",
                "127.0.0.1".parse().unwrap()
            )
            .is_err()
        );
        assert!(contains("10.1.2.3/8", "10.0.0.1".parse().unwrap()).is_err());
        assert!(contains("::/129", "::1".parse().unwrap()).is_err());
    }
}
