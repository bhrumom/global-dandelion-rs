# Offline Mini App implementation record

Native base: PR #1 aa6436315cde7b66868b0592d1c67b4590ebd1f9.
Spec: docs/specs/fabushi-miniapp.md. ADR: 0011.

The Web adapter calls the existing dandelion-packet encoder/decoder and exports
only bounded byte-array computation. Native source and 37-row migration status
remain unchanged. Portable mapping: C12 reuses exact packet implementation;
C03 uses user-selected Web File objects rather than native rooted scanning;
C19 adds an observer UI over local packet-verification results. C06..11, C15..18,
C20..23 native/Host adaptations remain pending for mini-app parity. No public
network sends, peer receipts or full replacement are claimed by this UI.

Owner selected LicenseRef-Proprietary on 2026-10-13. LICENSE covers original code;
third-party licenses remain separate distribution obligations.

Publication prerequisites: exact-head CI; committed Web dependency lockfile;
third-party notices; authorized publisher and managed signing; scan/review and
immutable catalog; real packaged Host offline/isolation/lifecycle acceptance.

Production read-only probe 2026-10-13: GET api.ombhrum.com/v1/extensions/trust
and /v1/extensions?kind=web-miniapp returned HTTP 404 with
x-fabushi-control-plane: mahayana-platform. The Node/JS route descriptions alone
therefore do not prove those routes exist on the active Rust control plane.
The unified device connector reports htch-runtime and macOS/Windows clients
offline; no actual installed-client acceptance is available in this snapshot.

The actual legacy Rust catalog at GET /v1/marketplace/plugins does return 200;
its public IDs do not include Global Dandelion. A POST with an empty manifest
to /v1/extensions/review/fabapp/sign returns 404, confirming that the protected
OMA signer route is absent, rather than merely waiting for authentication.
Legacy tar-gz/local-web listings cannot substitute for OMA signed .fabapp trust.

All new acceptance is pending. GitHub CI browser fixtures are preparation evidence,
not public trust approval or signed Fabushi installed-runtime acceptance.
