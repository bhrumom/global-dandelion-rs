# Bundled compatible receiver network

User confirmed compatible global receivers and required nodes to be built into
the Mini App. The immutable numeric receiver catalogue and CA trust are therefore
compiled into dandelion-network and delivered with the signed package. Startup
does not require a remote directory. Only the fixed HTTPS certificate issuer is
contacted to enroll a locally generated per-device CSR; private keys stay local.

The existing TLS 1.3/GDAN v2 data plane remains authoritative. The app probes exact
bundled endpoints using mutual authentication and a read-only clock exchange;
only successful nodes enter the short-lived prepared profile. User start opts
into payload transfer to those exact targets. No background payloads, shared
client key, arbitrary IP generation or Host business capability code.

Bootstrap has one bounded worker-owned subprocess and refreshes capability state
without replacing the backend or cancelling selections/transfers. Graceful close
cancels work and removes its private generation. Crash orphans are capped at eight;
robust stale-owner reclamation remains pending. Existing imported configurations
remain an advanced fallback. Local loopback stays available on network failure.

Parent: automatic-network-onboarding.md AN01..05. The production catalogue is
empty pending real consenting receiver deployment and reviewed CA/addresses.
The reachable Linux host failed disk reserve and later went offline. Issuer and
receiver deployment, rapid trust revocation, crash recovery, full worker/device
proof and genuine macOS AP lifecycle remain gates. This source increment must
not be advertised as a working global-send release or installed device proof.
