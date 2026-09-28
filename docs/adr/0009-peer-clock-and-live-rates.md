# ADR 0009: passive local and authenticated peer clock metadata

Status: implemented pending exact-revision verification. Spec C18/C19.

`platform time` observes local Unix time and proc uptime without host mutation.
`platform time --peer` uses configured exact authorized mTLS targets and trusted
certificates, not arbitrary public time APIs. A TLS control frame (length 1, byte 1)
requests GTIM + u64be Unix milliseconds. The existing metered stream accounts for
request, handshake and response limits; durable state leases are respected.
Read-only peer wall-clock offset is estimated with a monotonic round trip and
reported uncertainty. The peer is authenticated, not certified as an accurate UTC
authority. No runtime deadline, schedule or certificate verifier is bypassed or
corrected from this metadata. CI exercises this over the real mTLS fixture.

A pure rate estimator handles monotonic sample time, idle decay, counter reset and
zero-duration samples. TUI snapshots now show current accepted/verified rates,
without global worker maps or a blocking UI dependency in the transfer path.
