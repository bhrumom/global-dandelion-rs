# Global Dandelion in Fabushi — offline package specification

Status: active; release acceptance pending. Date: 2026-10-19.
Baseline: PR #1 at aa6436315cde7b66868b0592d1c67b4590ebd1f9,
native architecture Spec 2.1. Parent: Fabushi OMA-001/FPM-001 at
371b72f1cd20de03dc4dff9a15ed761054968093.

## Goal, current state and ownership

Deliver a complete signed offline Web mini app through Fabushi's existing
Marketplace. Reuse PR #1's actual Rust packet v2 implementation, rather than
creating an unrelated checksum demo or a second transfer engine. The current
native CLI and 37-row migration ledger remain authoritative and unchanged.
Native code completion and full-replacement hardware/release acceptance remain
separate. The package is a **cross-platform offline packet workbench**: one Web
bundle, three hosts. The full Linux CLI is not secretly executed/downloaded by the
Web surface.

Offline scope: user-selected files, bounded sequential packet construction,
packet integrity verification, progress/cancel, report export and source/permission
inspection. Network transfer, mounts, hotspot, service installation and native
update require a reviewed native Host/Connector adaptation before mini-app parity
can be claimed. They are reachable only through the declared tool contract below,
never mocked as successful, and never silently granted. Public listing cannot claim
full native feature parity.

Existing dandelion-core/dandelion-packet own domain and codec. A small
miniapp/wasm adapter compiles those same sources to WebAssembly; it owns only
browser bindings. miniapp/web owns Web-standard File APIs and rendering.
miniapp/web/host-tools.js owns the capability contract shared by all three hosts.
Fabushi Host owns trusted package installation, origins/account storage and
permissions; existing Backend Marketplace owns publisher/review/signing/catalog.
No second Registry, native runtime, shell bridge or raw CDP/filesystem grant.

## Requirements and contracts

| ID | Required observable behavior |
| --- | --- |
| GD-M01 | Cold-launch HTML/CSS/JS/Wasm from installed local bytes; no CDN, remote font, service worker, login or network request. |
| GD-M02 | At most 128 selected files and 1 GiB total; one 1,152-byte payload at a time; no directory scan or file-content persistence. |
| GD-M03 | Reuse the existing GDAN v2, 80-byte header/BLAKE3 codec; malformed/tampered/truncated packets rejected. Do not invent another wire protocol. |
| GD-M04 | Generation-fenced cancellation/new selection/close, bounded reports, visible reading/module/policy errors, literal text rendering and accessible progress. |
| GD-M05 | Offline results are local encode/decode verification, never UDP delivery, remote receipt, authentication, whole-file reconstruction or throughput claims. |
| GD-M06 | Export report only by explicit action; settings-only Web storage, visible quota failure, delete control. Host app/account isolation remains a package acceptance gate. |
| GD-M07 | Strict .fabapp v1/JCS/Ed25519 domain fabushi.fabapp.manifest.v1 followed by NUL, complete file SHA256/size/MIME table, packageClass catalog, no requested native capabilities, network deny. |
| GD-M08 | Managed existing catalog-authorized publisher key; no automatically generated production key. Scan/moderation/release signatures are separate from package signing. |
| GD-M09 | One bundle for Desktop, iOS and Android. Capability-bearing panels render only when the host advertises that capability in `fabushi.miniapp.tools.v1`; an undeclared capability hides the whole panel instead of degrading into an implicit grant. No per-platform feature claim until that platform has its own OMA policy and device proof. |
| GD-M10 | GitHub Actions only for build/test/package/browser acceptance, record exact SHA, lockfile, artifact digests/test counts; preserve existing native gates. |
| GD-M11 | Owner selected dual open-source distribution on 2026-10-19: `MIT OR Apache-2.0`, both texts shipped in the package; preserve third-party notices. |
| GD-M12 | Package bytes may be hosted off-registry (GitHub Releases). The signed release payload then carries the download `url`; the host accepts only allowlisted https hosts, verifies sha256/size after download, and treats a digest mismatch as failure rather than falling back. |
| GD-M13 | The signed Marketplace descriptor must declare `runtime.installMode=fabapp` and the same app/publisher identity as the package. A descriptor without this field is not installable on Desktop and must be rejected by release preparation before signing. |

Proposed identity org.fabushi.global-dandelion, publisher bhrumom,
next corrective release 0.2.1, platforms desktop/ios/android, minHostApi 1, runtime web,
entrypoint index.html.
These IDs require actual Marketplace namespace/key binding before publication.
Wasm exports encode_packet and verify_packet; only explicit bounded byte arrays,
u32 file/index, terminal flag and a 16-byte transfer ID cross the binding.
No arbitrary native pointer/callback/process/network/Host API is exposed.

## Failure, migration and implementation

Missing Wasm support/Host policy denies compilation => processing disabled with
clear status. File read failure/cancellation discards late results. Empty files
produce one terminal packet. Oversized inputs fail before reading. Report export
may contain filenames, never file contents or host paths. No implicit resume or
redispatch after reload. Cached/unsigned bundles cannot bypass installation trust.

Implement adapter and bundled UI, then Actions builds matching wasm-bindgen
bindings and performs codec/browser/package negative tests. Unsigned preparation
is .zip, never installable .fabapp. Test signing uses ephemeral keys and is
explicitly not production trust. Production .fabapp comes only from the existing
managed signer after release prerequisites. No signing secrets on PR workflows.
Before delivery, validate that the source descriptor contains the Host-required
`runtime.installMode=fabapp`, `runtime.appId=org.fabushi.global-dandelion`, and
`runtime.publisherId=bhrumom`; this metadata check does not alter the package
manifest or signing authority.

## Acceptance, publication and rollback

| Gate | Evidence needed |
| --- | --- |
| AC-M1 | Exact-head Rust/Wasm/browser tests proving M01..06; fresh context with network denied. |
| AC-M2 | Complete resources/signature/digests plus mutation/path/extra-file rejection tests. |
| AC-M3 | Owner-selected license, verified publisher binding, actual protected signer, scans/reviewer approval and immutable public catalog entry; the signed descriptor passes GD-M13. |
| AC-M4 | Real signed packaged Fabushi install, offline restart, two-account/app isolation, update/rollback/uninstall through existing owners (OMA AC-01..06/08..10). |
| AC-M5 | Native capability adaptation and real evidence before any full-feature mini-app claim; original migration/hardware gates unchanged. Each host proves its own capability set on device; the shared bundle proves the undeclared-capability path hides panels. |

All gates initially pending; lack of signer, approved registry, supported Host,
device or license is blocked, never pass. Rollout preview only with accurate
description; bad version uses existing Registry revocation and last-good Host
rollback. Never erase app user data on failed installation/update.
Record source SHA, Actions URLs, package and trust fingerprints, review IDs,
Host build/source and acceptance artifacts. No selected private data in telemetry.

## References

PR #1; docs/protocol-v2.md; docs/specs/global-dandelion-rust-architecture.md;
docs/migration/parity.json. Fabushi Desktop OMA/FPM specifications.
Fabushi Backend docs/specs/fabapp-manifest-signing-v1.md and existing
extension registry, artifact, signing and release owners; miniapp/RELEASE.md for
the GitHub-hosting pipeline. Latest user requests
on 2026-10-13: prioritize Global Dandelion and finish/list it in Marketplace.
