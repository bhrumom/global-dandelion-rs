# ADR 0004: authenticated external peers over TLS 1.3

Status: implemented pending exact-revision verification. Spec v2 C09-C12/C14.

Use maintained rustls and tokio-rustls with explicit ring provider and TLS 1.3 only,
mutual CA-verified certificates, explicit server name, ALPN global-dandelion/2 and
no early data. Connect only to exact numeric authorized SocketAddr values; the
TLS server name never triggers DNS destination expansion. WAN needs explicit
per-peer authorization, WAN enabled and an exact address allowlist. Listeners
outside loopback need explicit consent and allowed client IPs in addition to mTLS.

A shared, owned byte/rate meter wraps TCP BELOW TLS so handshake, ciphertext,
acknowledgements, shutdown and connect retries cannot bypass limits. Counters measure
socket payload bytes, not physical IP/TCP framing or retransmissions by the kernel.
The meter has an aggregate receive byte cap as well. No process-global counters.

Frame = u32 length + packet-v2. Receipt = GACK + BLAKE3(frame packet), protected by
TLS. End = zero frame length; GDON acknowledges successful stream completion.
This is an in-memory integrity receipt, not durable storage or a new cryptographic
protocol. TLS provides identity, confidentiality and record replay protection.
There are no ambiguous data retransmissions; a lost data receipt fails the session.
Only pre-payload connections may retry at most three times with bounded backoff.
Receiver tasks are bounded by JoinSet, timeout every handshake/frame, and are
aborted/joined on cancellation. No reflected unauthenticated plaintext response.

Credentials are bounded file inputs; private keys require private permissions,
regular-file/no-symlink/no-hardlink checks and are excluded from scanning. Production
keys are never stored in fixtures, argv, logs or snapshots. Tests generate ephemeral
CA/client/server material on disposable CI and transmit only over loopback.
