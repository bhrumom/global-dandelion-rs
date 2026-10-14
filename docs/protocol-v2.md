# Global Dandelion packet v2

All integers are unsigned big endian. Maximum datagram payload: 1,232 bytes.
Packet construction is specified by ADR 0001, not compatible with the Python
`u32 index + MD5 + data` framing.

| Offset | Bytes | Meaning |
| --- | ---: | --- |
| 0 | 4 | ASCII `GDAN` |
| 4 | 1 | Version `2` |
| 5 | 1 | Bit 0: terminal chunk. Other bits must be zero. |
| 6 | 2 | Header length: 80 |
| 8 | 16 | Per-run random transfer identifier from the OS RNG |
| 24 | 8 | File identifier, unique within this transfer |
| 32 | 8 | Zero-based chunk index |
| 40 | 4 | Payload length, 0..1,152 |
| 44 | 4 | Reserved: zero |
| 48 | 32 | BLAKE3 checksum of bytes 0..48 concatenated with payload |
| 80 | variable | Raw payload |

Length must match exactly; truncation, extension, unknown version/flags, nonzero
reserved fields and corruption are rejected. Empty payload is allowed only for a
terminal packet, permitting empty-file representation. Identifier ownership and
all integer fields are checksum-covered. Decode borrows the payload without
allocating a second payload. Encode reuses a caller-owned buffer.

This protocol does **not** supply filenames, file reconstruction, peer identity,
authenticated encryption, replay protection, retransmission, durable receipt,
resume or whole-file hashes. Those are explicit later protocol/application work.
A local verification receipt means the expected bytes were received and decoded
on the owned loopback receiver, not stored at a remote destination.

Tests: randomized round trips; malformed arbitrary inputs; bit corruption at every
byte; truncation at every offset; extension; maximum payload; exact end-to-end
loopback receipt. See `crates/dandelion-packet/src/lib.rs` and transport tests.
