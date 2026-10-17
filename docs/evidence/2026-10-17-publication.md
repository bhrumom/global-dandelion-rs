# Publication record: Global Dandelion offline preview 0.1.0

Date: 2026-10-17. Scope: the published web mini app only. This record does not
approve native LAN/WAN transfer, hotspot, system scanning or any full-feature
replacement claim, and it does not accept later Desktop source.

## Published entry

- Catalog id: `bhrumom/global-dandelion:web-miniapp`, origin public,
  trust domain `catalog-authorized`.
- Version 0.1.0, channel `preview`, `revoked: false`, minHostApi 1,
  platforms `["desktop"]`, requested capabilities `[]`,
  network policy `deny` with no allowed origins.
- Package: `desktop-fabapp`, 453093 bytes, SHA256
  `de10e02115415908f827723457817e51cb4f39aa5ca68798da824fecc483118a`.
- Signer: `fabushi-marketplace-managed-20261013-1`, trust epoch 1,
  public key fingerprint `b48c1dcadde5cac868ec0f9c03a9e9bf907548a68344ae315b650120b2ba06c5`.
- Source commit: `fce746baecc6bbd9f4b12d5862d886cf7f72eda5`, merged to main by
  PR #2 as `dc6dfae5068758942262cbcd71dc4d8a25f4ea64`.
- Protected review run: 38505804519 (`Review exact Global Dandelion offline preview`).

Public read endpoints:

- `GET https://api.ombhrum.com/v1/extensions?kind=web-miniapp`
- `GET https://api.ombhrum.com/v1/extensions/web-miniapp/bhrumom/global-dandelion`
- `GET https://api.ombhrum.com/v1/extensions/install/web-miniapp/bhrumom/global-dandelion/0.1.0`
- `GET https://api.ombhrum.com/v1/extensions/artifacts/de10e02115415908f827723457817e51cb4f39aa5ca68798da824fecc483118a`

## Independent re-verification of the published bytes

Performed against the artifact served by the registry, not a local rebuild:

| Check | Result |
| --- | --- |
| Artifact download by signed digest | HTTP 200, 453093 bytes, SHA256 matches the descriptor and install proof |
| Manifest Ed25519 signature | Verified with the public key from the published trust feed over domain `fabushi.fabapp.manifest.v1` plus NUL and canonical JSON |
| Resource table | 10/10 listed files match their SHA256 and byte length; no unlisted resource in the archive |
| Wasm codec | Published module instantiates; `max_payload_bytes` 1152; 5-byte payload encodes to an 85-byte frame and round-trips byte-identically |
| Tamper rejection | Truncated, oversized, trailing-byte and single-bit-flip frames are all rejected |

CI on the exact published source also passed the Rust/Wasm build, the adversarial
package tests and the network-denied browser contracts in the Offline Mini App
workflow.

## What remains open

- Installed-client acceptance for later Desktop source. The same immutable bytes
  passed the historical installed lifecycle run 38830168249; the Desktop factory
  A/B/A and promotion gates in `docs/evidence/2026-10-16-dandelion-factory-delivery.md`
  are still open and are not implied by this publication.
- Native capability parity: LAN/WAN transfer, hotspot, system scanning and
  service installation need a reviewed Host adaptation. The listing states the
  preview scope and does not claim them.

Rollback stays with the existing registry revoke/yank owner; no database edit and
no key rotation.
