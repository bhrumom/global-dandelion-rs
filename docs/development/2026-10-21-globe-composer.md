# Globe composer implementation campaign

User requested central 3D Earth, message/file input, mode/repeat controls and Rust
on all platforms; hotspot means authorized hotspot/LAN transmission (confirmed).
Spec: ../specs/globe-composer.md. Source baseline main ccbe267. Effort inherited
from session; no worker/model override, no paid-model acceptance. All builds and
executable tests run in Actions. A local rustup inventory unexpectedly started
syncing its directory override; that exact process was terminated, with no local
compilation or test run. No local toolchain installation is needed for this work.

Rust/Wasm computes bounded sphere, great-circle and surface-ring geometry, and
executes real GDAN v2 packet verification for local text/files. WebGL draws local
geometry; no online map/imagery. Coordinates are explicit display-only metadata.
Native picker snapshots exactly selected regular files into app-private staging;
text composition uses the same bounded source path. Releases/cancel/close retain
ownership until readers stop, then delete only owned staging roots.

The Rust CLI now distinguishes explicit local-loopback override from direct LAN
TLS filtering. LAN mode clears WAN pools/allowlist, retains only already-authorized
private direct peers and still requires mTLS. An imported TLS configuration no
longer makes the local-labelled tool accidentally use remote transport. Legacy
UDP mode remains distinct from the composer's labelled in-memory packet loopback.

Acceptance remains pending exact-head Actions, signer/catalog/install and mobile
native adapter/device proof. Android self-contained packaging must include nested
composer/globe modules. Existing mobile bridges with empty target pools do not
prove native transmission. No blanket full-function or physical-energy claim.

Actions recovery: be1939f geometry tests passed but Clippy required as_chunks;
3b58e26 passed Linux x86/ARM native quality, dependency audit and required gates.
Browser detected genuine 390px selector overflow (GC01 implementation bug); fix
wraps controls without hiding page overflow. Native staging/send/release reached
the cleanup assertion, where the fixture omitted fs readdir/symlink imports
(test flaw). Preserve those proofs and correct the focused failures.
Receipt progress is now a bounded opt-in Rust observer, separate from process
admission; unchanged counters cannot keep a flying route animated indefinitely.

User requested installation on this Mac. Release preparation now uses 0.4.1;
0.4.0 remains reproducible and immutable. Same native-runtime grants and
managed trust root; signing/catalog/install/live UI remain pending until observed.
