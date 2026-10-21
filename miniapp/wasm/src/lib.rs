//! Web-standard byte-array adapter over the existing native GDAN v2 codec.
//! No sockets, host paths, lifecycle authority or native privileges are exposed.
use dandelion_core::{Chunk, MAX_DATAGRAM_BYTES, MAX_PAYLOAD_BYTES};
use wasm_bindgen::prelude::*;
mod globe;
pub use globe::{globe_mesh, globe_ripple, globe_route};

#[wasm_bindgen]
pub fn max_payload_bytes() -> u32 {
    MAX_PAYLOAD_BYTES as u32
}

/// Encodes one bounded chunk using the same implementation as the native CLI.
#[wasm_bindgen]
pub fn encode_packet(
    payload: &[u8],
    transfer_id: &[u8],
    file_id: u32,
    index: u32,
    last: bool,
) -> Result<Vec<u8>, JsValue> {
    if payload.len() > MAX_PAYLOAD_BYTES || transfer_id.len() != 16 {
        return Err(JsValue::from_str("invalid bounded packet input"));
    }
    let mut identity = [0; 16];
    identity.copy_from_slice(transfer_id);
    let chunk = Chunk {
        transfer_id: identity,
        file_id: u64::from(file_id),
        index: u64::from(index),
        last,
        payload: payload.to_vec(),
    };
    let mut encoded = Vec::with_capacity(MAX_DATAGRAM_BYTES);
    dandelion_packet::encode(&chunk, &mut encoded)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    Ok(encoded)
}

/// Returns only the payload of a valid native v2 packet; corruption is an error.
#[wasm_bindgen]
pub fn verify_packet(packet: &[u8]) -> Result<Vec<u8>, JsValue> {
    dandelion_packet::decode(packet)
        .map(|decoded| decoded.payload.to_vec())
        .map_err(|error| JsValue::from_str(&error.to_string()))
}
