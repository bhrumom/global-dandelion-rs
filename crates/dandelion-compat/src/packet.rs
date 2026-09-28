//! Exact Python `!I index + MD5(payload) + payload` layout for inspection and
//! controlled baseline comparison ONLY. MD5 is not cryptographic authenticity.
use dandelion_core::{Error, Result};
use md5::{Digest, Md5};
use serde::Serialize;
pub const MAX_PAYLOAD: usize = 60_000;
pub fn encode(index: u32, payload: &[u8]) -> Result<Vec<u8>> {
    if payload.len() > MAX_PAYLOAD {
        return Err(Error::Protocol("legacy payload too large"));
    }
    let mut out = Vec::with_capacity(20 + payload.len());
    out.extend_from_slice(&index.to_be_bytes());
    out.extend_from_slice(&Md5::digest(payload));
    out.extend_from_slice(payload);
    Ok(out)
}
#[derive(Debug, Serialize)]
pub struct Inspected {
    pub index: u32,
    pub payload_bytes: usize,
    pub legacy_md5_checksum: bool,
    pub authenticated: bool,
}
pub fn inspect(bytes: &[u8]) -> Result<Inspected> {
    if !(20..=MAX_PAYLOAD + 20).contains(&bytes.len()) {
        return Err(Error::Protocol("legacy packet length invalid"));
    }
    if Md5::digest(&bytes[20..]).as_slice() != &bytes[4..20] {
        return Err(Error::Protocol("legacy MD5 mismatch"));
    }
    let mut index = [0; 4];
    index.copy_from_slice(&bytes[..4]);
    Ok(Inspected {
        index: u32::from_be_bytes(index),
        payload_bytes: bytes.len() - 20,
        legacy_md5_checksum: true,
        authenticated: false,
    })
}
#[derive(Debug, Serialize)]
pub struct Measurement {
    pub packets: u32,
    pub payload_bytes: usize,
    pub elapsed_seconds: f64,
    pub final_packet_md5: String,
    pub wire_format: &'static str,
    pub network_bytes: u64,
}
pub fn benchmark(iterations: u32, chunk_bytes: usize) -> Result<Measurement> {
    if !(1..=1_000_000).contains(&iterations) || !(1..=MAX_PAYLOAD).contains(&chunk_bytes) {
        return Err(Error::Config(
            "benchmark bounds: 1..1000000 iterations, 1..60000 payload bytes".into(),
        ));
    }
    let payload = vec![0x5a; chunk_bytes];
    let start = std::time::Instant::now();
    let mut final_packet = Vec::new();
    for index in 0..iterations {
        final_packet = std::hint::black_box(encode(index, std::hint::black_box(&payload))?);
    }
    let elapsed_seconds = start.elapsed().as_secs_f64();
    let final_packet_md5 = Md5::digest(&final_packet)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok(Measurement {
        packets: iterations,
        payload_bytes: chunk_bytes,
        elapsed_seconds,
        final_packet_md5,
        wire_format: "python-u32be-md5-payload",
        network_bytes: 0,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_python_golden_packet() {
        let bytes = encode(7, b"abc").unwrap();
        assert_eq!(&bytes[..4], &[0, 0, 0, 7]);
        assert_eq!(
            &bytes[4..20],
            &[
                0x90, 0x01, 0x50, 0x98, 0x3c, 0xd2, 0x4f, 0xb0, 0xd6, 0x96, 0x3f, 0x7d, 0x28, 0xe1,
                0x7f, 0x72
            ]
        );
        assert_eq!(&bytes[20..], b"abc");
        let p = inspect(&bytes).unwrap();
        assert_eq!(p.index, 7);
        assert!(!p.authenticated);
    }
    #[test]
    fn corruption_and_bounds_fail() {
        let mut p = encode(1, b"x").unwrap();
        p[20] ^= 1;
        assert!(inspect(&p).is_err());
        assert!(inspect(&[0; 19]).is_err());
        assert!(encode(0, &vec![0; MAX_PAYLOAD + 1]).is_err());
    }
}
