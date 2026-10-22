# Bundled receiver network preparation

User confirmed compatible receiving nodes and then required the nodes to be built
into the Mini App. The immutable numeric endpoint catalogue and CA are therefore
compiled into dandelion-network. Native Rust generates a private device identity,
enrolls only its CSR, validates the returned client certificate/key/CA and probes
TLS 1.3/GDAN clock exchange before writing a private prepared profile. The CLI
entry is `network-prepare --directory <private-app-data> --json`.

Tests cover invalid/private/stale/duplicate directories, bundled seed limits,
wrong key/CA/expired/client-versus-server certificates, privilege-bearing CSRs,
forged enrollment responses, private identity permissions/symlinks and actual
loopback mTLS/GDAN exchange with zero payload. CI must run these; local compilation
and tests are prohibited by repository policy.

Current delivery is NOT production-ready: receivers.json is empty because no real
receiver has been deployed. The online Linux ARM64 device bhrum2 has 2,210,902,016
free bytes versus a 2,366,063,412 reserve, failing even a 64 MiB deployment. No
server writes, cleanup or port exposure were performed. HTTPS issuer deployment,
worker automatic initialization, generation ownership/cleanup, UI integration,
macOS AP lifecycle and signed installation remain pending. Do not publish a new
working-network release or claim device acceptance from this source increment.
