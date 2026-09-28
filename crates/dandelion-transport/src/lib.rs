//! Transport port and an owned, connected UDP loopback verification adapter.
//! There are no detached receive tasks, public destinations, or global sockets.
use dandelion_core::{Error, MAX_DATAGRAM_BYTES, Result};
use std::{future::Future, time::Duration};
use tokio::net::UdpSocket;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub struct Receipt {
    pub accepted_wire_bytes: u64,
    pub verified_payload_bytes: u64,
}

pub trait Transport: Send {
    fn wire_cost(&self, packet: &[u8]) -> u64 {
        packet.len() as u64
    }
    fn transmit(
        &mut self,
        packet: &[u8],
        cancel: &CancellationToken,
    ) -> impl Future<Output = Result<Receipt>> + Send;
}

/// Deliberately stop-and-verify: correctness baseline, not a throughput claim.
/// The local receiver validates each packet before accepting the next one.
pub struct Loopback {
    sender: UdpSocket,
    receiver: UdpSocket,
    buffer: [u8; MAX_DATAGRAM_BYTES + 1],
}
impl Loopback {
    pub async fn bind() -> Result<Self> {
        let sender = UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let receiver = UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        sender.connect(receiver.local_addr()?).await?;
        receiver.connect(sender.local_addr()?).await?;
        Ok(Self {
            sender,
            receiver,
            buffer: [0; MAX_DATAGRAM_BYTES + 1],
        })
    }
}
impl Transport for Loopback {
    async fn transmit(&mut self, packet: &[u8], cancel: &CancellationToken) -> Result<Receipt> {
        dandelion_packet::decode(packet)?;
        let exchange = async {
            let sent = self.sender.send(packet).await?;
            if sent != packet.len() {
                return Err(Error::Protocol("partial UDP send"));
            }
            let received = self.receiver.recv(&mut self.buffer).await?;
            let decoded = dandelion_packet::decode(&self.buffer[..received])?;
            if &self.buffer[..received] != packet {
                return Err(Error::Protocol(
                    "received frame differs from transmitted frame",
                ));
            }
            Ok(Receipt {
                accepted_wire_bytes: sent as u64,
                verified_payload_bytes: decoded.payload.len() as u64,
            })
        };
        tokio::select! { biased;
            _ = cancel.cancelled() => Err(Error::Cancelled),
            result = tokio::time::timeout(Duration::from_secs(1), exchange) => result.map_err(|_| Error::VerifierTimeout)?,
        }
    }
}

/// Read-only surface pass: decode and verify streamed chunks, no socket opened.
pub struct ReadVerifier;
impl Transport for ReadVerifier {
    fn wire_cost(&self, _packet: &[u8]) -> u64 {
        0
    }
    async fn transmit(&mut self, packet: &[u8], cancel: &CancellationToken) -> Result<Receipt> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let frame = dandelion_packet::decode(packet)?;
        Ok(Receipt {
            accepted_wire_bytes: 0,
            verified_payload_bytes: frame.payload.len() as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dandelion_core::Chunk;
    #[tokio::test]
    async fn actual_udp_is_verified_and_cancelable() {
        let mut local = Loopback::bind().await.unwrap();
        let cancel = CancellationToken::new();
        let mut bytes = Vec::new();
        for index in 0..20 {
            dandelion_packet::encode(
                &Chunk {
                    transfer_id: [1; 16],
                    file_id: 1,
                    index,
                    last: index == 19,
                    payload: b"hello".to_vec(),
                },
                &mut bytes,
            )
            .unwrap();
            let receipt = local.transmit(&bytes, &cancel).await.unwrap();
            assert_eq!(receipt.verified_payload_bytes, 5);
            assert_eq!(receipt.accepted_wire_bytes, 85);
        }
        cancel.cancel();
        assert!(matches!(
            local.transmit(&bytes, &cancel).await,
            Err(Error::Cancelled)
        ));
    }
}
