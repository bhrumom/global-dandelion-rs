//! Version 2, big-endian, bounded datagram framing. BLAKE3 is a checksum,
//! not peer authentication or encryption. See docs/protocol-v2.md.
use dandelion_core::{Chunk, Error, HEADER_BYTES, MAX_DATAGRAM_BYTES, MAX_PAYLOAD_BYTES, Result};

#[derive(Debug, PartialEq, Eq)]
pub struct Decoded<'a> {
    pub transfer_id: [u8; 16],
    pub file_id: u64,
    pub index: u64,
    pub last: bool,
    pub payload: &'a [u8],
}

/// Reuses the caller-owned buffer; no destination-specific packet allocation.
pub fn encode(chunk: &Chunk, out: &mut Vec<u8>) -> Result<()> {
    if chunk.payload.len() > MAX_PAYLOAD_BYTES {
        return Err(Error::Protocol("payload too large"));
    }
    if chunk.payload.is_empty() && !chunk.last {
        return Err(Error::Protocol("empty nonterminal chunk"));
    }
    out.clear();
    out.extend_from_slice(b"GDAN");
    out.extend_from_slice(&[2, u8::from(chunk.last)]);
    out.extend_from_slice(&(HEADER_BYTES as u16).to_be_bytes());
    out.extend_from_slice(&chunk.transfer_id);
    out.extend_from_slice(&chunk.file_id.to_be_bytes());
    out.extend_from_slice(&chunk.index.to_be_bytes());
    out.extend_from_slice(&(chunk.payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&[0; 4]);
    let mut hash = blake3::Hasher::new();
    hash.update(out);
    hash.update(&chunk.payload);
    out.extend_from_slice(hash.finalize().as_bytes());
    out.extend_from_slice(&chunk.payload);
    Ok(())
}

/// Length is validated before slicing. Unknown versions/flags are rejected.
pub fn decode(bytes: &[u8]) -> Result<Decoded<'_>> {
    if !(HEADER_BYTES..=MAX_DATAGRAM_BYTES).contains(&bytes.len()) {
        return Err(Error::Protocol("datagram length"));
    }
    if &bytes[..4] != b"GDAN" || bytes[4] != 2 {
        return Err(Error::Protocol("magic or version"));
    }
    if bytes[5] & !1 != 0
        || bytes[6..8] != (HEADER_BYTES as u16).to_be_bytes()
        || bytes[44..48] != [0; 4]
    {
        return Err(Error::Protocol("flags, header length, or reserved field"));
    }
    let mut length_bytes = [0; 4];
    length_bytes.copy_from_slice(&bytes[40..44]);
    let length = u32::from_be_bytes(length_bytes) as usize;
    if length != bytes.len() - HEADER_BYTES {
        return Err(Error::Protocol("payload length mismatch"));
    }
    let last = bytes[5] == 1;
    if length == 0 && !last {
        return Err(Error::Protocol("empty nonterminal chunk"));
    }
    let mut hash = blake3::Hasher::new();
    hash.update(&bytes[..48]);
    hash.update(&bytes[HEADER_BYTES..]);
    if hash.finalize().as_bytes() != &bytes[48..80] {
        return Err(Error::Protocol("checksum mismatch"));
    }
    let mut transfer_id = [0; 16];
    transfer_id.copy_from_slice(&bytes[8..24]);
    let mut file_id = [0; 8];
    file_id.copy_from_slice(&bytes[24..32]);
    let mut index = [0; 8];
    index.copy_from_slice(&bytes[32..40]);
    Ok(Decoded {
        transfer_id,
        file_id: u64::from_be_bytes(file_id),
        index: u64::from_be_bytes(index),
        last,
        payload: &bytes[HEADER_BYTES..],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    proptest! {
        #[test]
        fn round_trip(payload in prop::collection::vec(any::<u8>(), 0..=MAX_PAYLOAD_BYTES), file_id in any::<u64>(), index in any::<u64>()) {
            let chunk = Chunk { transfer_id: [7;16], file_id, index, last: true, payload };
            let mut buf = Vec::new(); encode(&chunk, &mut buf).unwrap();
            let frame = decode(&buf).unwrap();
            prop_assert_eq!(frame.payload, chunk.payload);
            prop_assert_eq!(frame.file_id, file_id);
            prop_assert_eq!(frame.index, index);
            prop_assert_eq!(frame.transfer_id, [7;16]);
        }
        #[test]
        fn arbitrary_input_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..2048)) { let _ = decode(&bytes); }
    }
    #[test]
    fn every_single_byte_corruption_is_detected() {
        let chunk = Chunk {
            transfer_id: [9; 16],
            file_id: 3,
            index: 2,
            last: true,
            payload: b"fixture".to_vec(),
        };
        let mut bytes = Vec::new();
        encode(&chunk, &mut bytes).unwrap();
        for i in 0..bytes.len() {
            let mut bad = bytes.clone();
            bad[i] ^= 1;
            assert!(decode(&bad).is_err(), "undetected byte {i}");
        }
        for len in 0..bytes.len() {
            assert!(decode(&bytes[..len]).is_err());
        }
        bytes.push(0);
        assert!(decode(&bytes).is_err());
    }
    #[test]
    fn oversize_rejected() {
        let chunk = Chunk {
            transfer_id: [0; 16],
            file_id: 1,
            index: 0,
            last: true,
            payload: vec![0; MAX_PAYLOAD_BYTES + 1],
        };
        assert!(encode(&chunk, &mut Vec::new()).is_err());
    }
}
