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

AN02: global target preparation is automatic once its target model is settled.
The user has been asked whether this means compatible global receiving nodes or
arbitrary public IP generation. Until answered, do not invent a node registry,
claim public IPs run receivers, or widen target authorization. For receiving
nodes, discovery must bind identity, trust, endpoint, freshness and revocation;
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
