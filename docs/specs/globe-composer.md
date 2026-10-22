# Global Dandelion globe composer

Status: implementation, 2026-10-21. User sketch and correction are authoritative:
central 3D Earth, bottom message/file composer with + picker, mode and repeat
selectors above it. Modes: global sending, local rotation (local loopback), hotspot
field (authorized local hotspot/LAN transmission, confirmed by user). All three
installed mobile/desktop packages are required; absent native capability remains
an incomplete delivery gate, never a simulated successful transfer.

## Owners and architecture

Retain package-owned Rust core/packet/transport and existing generic Host grants.
No Dandelion business implementation in Fabushi Host, no extra signer or implicit
network grant. Rust/Wasm owns bounded sphere/route/ripple geometry and existing
GDAN packet verification. Browser owns WebGL drawing, pointer/file/text UI and
existing opaque tool calls. No CDN, map token, remote texture or fetch for maps.
The surface calls animated rings "场能波澜" as operation visualization, not a
measurement of physical energy or proof of wireless radiation.

## Requirements

- GC01: dark responsive globe-first layout follows sketch. Touch/drag rotation,
  accessible mode/repeat selectors, text input, + file picker, explicit start/stop,
  status and bounded statistics. Remove the legacy workbench from the document.
  Settings above the composer contain speed, location and authorized targets.
  Native diagnostic APIs remain. Touch targets >=44px, safe-area insets, reduced-motion and hidden-page
  pause, WebGL failure fallback. No persistent file/text/location data.
- GC02: Rust generates finite bounded 3D sphere positions, elevated great-circle
  routes and on-sphere ripple geometry. Validate coordinates and counts, handle
  coincident/antipodal endpoints without NaN, bound GPU buffers and cap frame rate.
  Geographical land silhouette is coarse illustrative geometry, no accuracy claim.
- GC03: local mode processes text and explicitly selected files with the existing
  Rust GDAN v2 encode/decode and BLAKE3 verification on every pass, including empty
  files. When the installed package announces local native transport, the composer uses
  its actual Rust UDP loopback. Otherwise label the Web/Wasm path as in-memory
  protocol loopback; this fallback is not full mobile native parity. Single pass or
  continuous until stop/error/budget, without a repeat-count selector. Preserve native
  lifetime network budgets; never auto-retry or reset budgets. Cooperative pacing,
  no overlapping work, cancellation generation fence and unload cleanup.
- GC04: global sends only to configured authenticated targets through the installed
  package. Hotspot uses configured class=lan direct private-IP peers only (class=private
  VPN/VPS peers are excluded), never creates/adopts public
  targets or silently enables WAN. Local mode must never inherit a remote TLS
  configuration. Native routing must enforce selected mode, not just its UI label.
- GC05: + selects actual files (not their parent directory); do not widen grants.
  For native transfer, copy bounded selected regular files through no-follow handles
  into an owned private staging root, verify mutation stamps, reserve disk, discard
  on release/cancel/close. No file path or certificate crosses into WebView. Text
  is encoded as an explicit in-memory file for local mode; native support must be
  truthful if its transport has no text-ingestion port yet.
- GC06: request location only on explicit gesture; offer manual lat/lon when denied.
  Never invent the device location or derive peer locations from random IPs.
  User-entered endpoint map coordinates are display metadata, never authorization.
  An opt-in bounded NDJSON progress observer samples real Rust receipt counters
  at most once per second until cancellation, retaining one current frame, leaving default CLI JSON unchanged.
  Flying trajectories require receipt-counter progress, not mere process admission.
  Routes require actual running/completed operation evidence plus known endpoints;
  distinguish active, confirmed, failed and cancelled. Stop visualization when
  operation stops. Missing coordinates display status rather than fake global paths.
- GC07: include every new module in both ordinary signed-resource preparation and
  Android self-contained HTML. Same Rust/Wasm code and layout on mobile. Full mobile
  transfer requires package-owned native integration and separate Android/iOS device
  proof; existing bridges that announce no targets must report unavailable clearly.
  An offline browser or contract fixture does not satisfy full mobile acceptance.

## Dependency remediation

The first exact-source Actions audit surfaced RUSTSEC-2026-0382/0383 in the
existing lockfile. Resolve only rustls-webpki to 0.103.16 and webpki-root-certs
to 1.0.10 (or compatible patched releases); preserve audit/license gates and
re-run real authenticated TLS acceptance after the lockfile change.

## Verification and delivery

Actions only: Rust geometry invariants, real Wasm/browser file+text/repeat/cancel,
no off-origin requests, pointer interaction, 390px/desktop/reduced-motion and
unavailable/malformed transport states; retain package signature/adversarial tests.
Real native fixture sends only to ephemeral authorized localhost peers. Test exact
selected-file isolation, imported remote config/local-mode distinction and LAN
fencing. Preserve CLI JSON contract and original migration ledger. Record exact
source/run/screenshot/package digest, then protected signing/catalog installation
and physical mobile tests as separate gates. No production secrets or paid models.

Rollback restores prior immutable package via existing marketplace. No in-place
mutation of 0.4.0 signed bytes. A release candidate needs a new version and real
signer/review proof; unsigned CI resources are not installable releases.

References: fabushi-miniapp.md; macos-installed-native-miniapp.md;
native-apple-core.md; global-dandelion-rust-architecture.md; user sketch in chat.

Mac update 2026-10-21: deliver immutable 0.4.1 through the protected managed
signer and approved catalog, then update this Mac through the existing Host.
No Host business code change or new permission scope. Verify installed version
and actual globe UI separately from CI; retain user data and 0.4.0 rollback.

## 2026-10-22 correction (0.4.2)

GC08: looping is continuous, not 10000 passes. Native sending uses one cancellable
Rust watch process without a fixed process-duration deadline. Ordinary commands
retain timeouts. Keep bounded concurrency, frame/output memory, graceful stop,
account/close cancellation and authorized lifetime traffic budgets. Progress has
no sample-count stop and retains only its latest complete frame and final receipt.
Wasm local loops validate counter overflow, source limits and cooperative pacing.

GC09: settings above the input expose independent local/global/hotspot positive
integer KiB/s speeds, converted to bytes/s for the Rust monotonic limiter. The
Wasm fallback uses monotonic pacing. Settings are locked during operations.
Configuration import and authorized targets remain accessible here; remove the
duplicate legacy UI and its handlers rather than hiding it.

GC10: global reach means authorized compatible receivers anywhere in the world.
Keep mTLS, WAN opt-in and explicit target policy; do not enumerate or stream to
arbitrary Internet IPs. Actions verify continuous cancellation, pacing, watch
arguments, bounded long-stream retention, removed legacy DOM and mobile layout.
0.4.2 retains package ownership and grants. 0.4.1 is immutable; signing/catalog
and installed proof remain independent gates.

GC11 (0.4.3, installed Mac observation): the existing Host unconditionally
disables GPU acceleration, so actual 0.4.2 WebGL creation fails despite Actions
WebGL passing. Keep the Host unchanged. Provide package-owned Canvas2D software
projection of the same Rust-generated 3D sphere, route and ripple coordinates.
Bound geometry/DPR/resolution, cap active frames to 15 fps, pause when hidden,
respect reduced motion, retain pointer/keyboard rotation and receipt-driven
overlays. No textures/CDN/network or fake receipt animation. Test real Canvas2D
with WebGL unavailable, pixels, rotation and lifecycle in Actions, then confirm
the installed Mac separately. Preserve immutable 0.4.2 and existing native grants.

2026-10-22 latest correction: automatic initialization/target onboarding and
actual hotspot enable are now required; see automatic-network-onboarding.md.
The previous existing-LAN-only scope is insufficient. These changes are pending
implementation and must not be claimed as delivered by 0.4.3.
