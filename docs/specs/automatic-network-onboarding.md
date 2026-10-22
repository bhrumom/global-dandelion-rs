# Automatic Mini App network onboarding

Status: required design correction, 2026-10-22; implementation not complete.
Latest user instruction supersedes the earlier join-existing-LAN-only hotspot UI:
entering the app initializes configuration; the program prepares global targets;
clicking enable hotspot starts the actual hotspot without manual file import.

## Ownership and behavior

AN01: package-owned Rust initializes private configuration, identity/trust material
and local endpoint with atomic writes, disk preflight and bounded state. No Host
business logic, shared credentials, fabricated successful readiness or need to
manually import TOML for the normal flow. Existing profile import remains optional.
Initialization does not read user files or start sending file payloads.

AN02: user confirmed automatic discovery of compatible global receiving nodes.
Do not generate arbitrary public IPs. The signed Mini App binary bundles only
operator-enrolled, consenting, compatible receiver addresses and CA trust. No
mandatory remote directory is required for startup discovery. For receiving nodes, discovery must bind identity, trust, endpoint, freshness and revocation;
only verified reachable registered nodes become sendable. Discovery is bounded,
cancellable and cannot silently publish a local listener to the Internet.

AN03: genuine macOS hotspot lifecycle is required, separate from transferring on
an existing LAN. Inspect usable adapters/uplink first; show and request required
OS permission on the explicit enable action, automatically select supported
settings, apply and verify actual hotspot readiness. A settings link, interface
label or running process is not proof of an access point. Never silently edit
privileged system files, bypass admin dialogs or disable network security.

AN04: preserve initial network state and owned changes for rollback on stop/error;
never shut down another application's existing sharing session. Explain and
confirm any disconnection before repurposing an active Wi-Fi interface. Offline
local hotspot and Internet Sharing are distinct; do not reject local hotspot
solely because there is no wired Internet uplink without actual platform proof.

AN05: after hotspot readiness, automatically discover/enroll compatible local
receivers according to the selected trust policy, then expose ready/unavailable
with a concrete reason. Joining a hotspot does not itself authorize file receipt.
Native methods, secrets, sockets and OS configuration remain package-owned;
WebView receives bounded status/opaque IDs. No unsolicited payload transmission.

## Current evidence and gaps

Source inspected: installed 0.4.3 source 2a267105c6621cbfa4696a1ae6075599f737cb75.
profile.ts starts with loopback and creates remote pools only after imported TLS
configuration. commands.rs explicitly rejects hotspot on non-Linux platforms;
the shipped native profile does not enable a macOS hotspot adapter. Therefore
neither automatic global onboarding nor macOS hotspot creation is delivered.
Local observation: default route en1 (Wi-Fi); en0/en4/en5/en6 have inactive links
and no IPv4. This is inventory, not proof that offline hotspot is impossible.

## Acceptance

Actions: fresh/existing/corrupt private profile; cancellation, bounded retries,
expired/untrusted/revoked discovery and authenticated real local two-node transfer;
no file payload before start, no arbitrary public-network test traffic, secret
redaction, mocked OS failure/rollback. Real Mac: OS grant, actual hotspot visible
to a second authorized device, transfer receipts, stop and network restoration.
Global: actual registered receiver and exact deployed node-registry evidence,
then signed package/catalog/install and real-device acceptance. Keep each gate
pending until observed; do not advertise the present 0.4.3 as fulfilling AN01..05.

Reference: https://support.apple.com/en-au/guide/mac-help/mchlp1540/27/mac/27
Apple documents selecting sharing source/output and Wi-Fi network/security;
that guide does not establish a public unattended programmatic hotspot API.


## Bundled receiver implementation

A package-owned Rust adapter includes a bounded catalogue of up to 32 exact
numeric receiver addresses/server names and their CA trust in the signed binary.
No automatic DNS/IP expansion or mandatory remote directory lookup is allowed.
The client generates its own private identity and CSR in Rust; only the CSR is
sent to the pinned canonical HTTPS issuer at api.ombhrum.com, without redirects.
The private key never leaves the app directory. The authority must issue
short-lived client-only certificates, replace untrusted CSR extensions with a
fixed client policy, and rate-limit enrollment. No shared client key, trust-all
verifier, production test identity or fabricated address.

Issuer deployment and managed receiver operations belong to the service backend;
packet/transport/identity/catalogue/private profile logic belongs to the Mini App
Rust package. Use existing GDAN v2, TLS 1.3 mutual authentication, receipts, pacing
and lifetime budgets. A prepared configuration does not prove readiness: probe
the selected receiver's authenticated protocol before exposing it as ready. A
bounded local lease requires renewed probing after expiry. Compromised node trust
must be removed through a signed package update; rapid revocation delivery and
real certificate lifecycle deployment are still pending.

Bootstrap is cancellable and bounded; initial local use must not wait for network
availability. Never cancel user transfers merely because background discovery
finishes. Re-read capabilities after preparation; do not freeze an initial empty
capability set for the lifetime of the page. Failures show retry state and cause;
no indefinite model-driven retries. Existing manually configured profiles remain
available as an advanced fallback, with no default requirement to import files.

2026-10-22 infrastructure inspection: account device bhrum2 was online Linux
ARM64; a later connection attempt reported it offline. Root filesystem free 2210902016 bytes, required reserve 2366063412 bytes;
64 MiB deployment preflight fails. No server files were written or removed.
Implement/test in Actions; deployment must wait for restored reserve or another
suitable source host, not consume the remaining disk or remove unrelated data.

## Provisioning gate

The latest user instruction makes the immutable receiver catalogue and its trust
anchors part of the installed Mini App package. Startup must not require a remote
directory lookup to obtain initial addresses. Rust includes the bounded catalogue
in its binary, validates exact numeric endpoints, and performs authenticated
protocol probes. No node count, location or readiness may be invented. The current
catalogue is deliberately empty until a consenting receiver is actually deployed
and its CA/address are reviewed. This is a deployment blocker, not a usable global
network. Certificate enrollment can contact the fixed managed issuer; private keys
remain local. A remote directory must not be a mandatory initial dependency.
