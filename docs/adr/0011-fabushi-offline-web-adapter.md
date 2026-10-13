# ADR-0011: Browser binding over the existing packet codec

Status: accepted for implementation; public release acceptance pending.
Date: 2026-10-13. Authority: latest user request to deliver a Fabushi offline app.

Use a narrowly scoped WebAssembly adapter over the existing pure
dandelion-core/dandelion-packet code at PR #1. Browser File selection replaces
native SourceAccess only in this Web interface. Native CLI, transports, service,
state, hardware gates and existing wire format remain unchanged.

The adapter has a separate Cargo workspace under miniapp/wasm so Linux-only
Tokio/rustix/hotspot dependencies cannot enter the browser package. This is a
build-target adapter in the same repository, not a shared runtime/Core product.
Dependency lockfile and generated JS/Wasm are captured by Actions; publication
requires committed reproducible inputs and exact-head evidence.

wasm-bindgen generates the numerical/byte-array ABI; application Rust retains
unsafe_code=forbid. No manual pointer exports, native binaries, network or shell
bridge. Local packet checks do not prove network delivery or full native parity.
OMA-001 installation, publisher trust, sandbox and policy gates still apply.
