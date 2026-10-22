# Automatic compatible receiver network

User confirmed globally located compatible receivers, not random Internet IPs.
Use one managed HTTPS directory/CSR enrollment control plane, per-device private
keys and short-lived client certificates, with the existing TLS 1.3/GDAN v2 data
plane. dandelion-network owns validation and identity/enrollment; app injects
storage and verified transport. Backend owns managed directory, rate-limited
issuer/proxy and receiver operations. No Host business code or shared client key.

Directory expiry, exact node endpoints and CA identity bind the prepared profile.
No payload is sent during bootstrap. Receiver readiness requires an authenticated
protocol response. Start explicitly opts into transfer to these registered nodes;
WAN remains closed outside exact selected endpoints. Server deployment additionally
requires free disk reserve, reviewed network exposure and actual receiving proof.

Parent: automatic-network-onboarding.md AN01..05. Current implementation and
production/device proof remain pending. Preserve existing manually imported
profiles and local loopback when no directory/receiver is available.

Superseding user clarification: initial receiver addresses and CA trust belong in
the signed Mini App binary, not a required remote directory. The HTTPS issuer only
enrolls the locally generated identity. `receivers.json` is empty pending real
receiver deployment; the parser fails closed, and this revision is not a working
global-send release. Bundled addresses still require live authenticated probes.
