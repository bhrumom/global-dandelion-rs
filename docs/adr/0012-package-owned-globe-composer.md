# ADR 0012: package-owned Rust globe composer

Status: implementation; exact-source acceptance pending. Spec GC01..07.

Use existing Rust/Wasm adapter for finite sphere/route/ripple geometry and packet
loopback. Keep WebGL and pointer/text/file controls as browser adapters. Ship every
module in the signed resource table and Android inline import map; no remote maps,
assets, telemetry, geolocation guesses or second protocol.

The original Rust engine works on roots. To support the explicit + file picker,
the package snapshots only selected regular handles into a bounded private root;
it must never send their parent directories. Native text composition copies into
a fresh root. Keep reader leases until the exact child settles, then remove owned
staging after release/close. Serial staging admission prevents free-space checks
being overcommitted by parallel copies. Inputs and locations are not preferences.

UI modes must change data-plane policy. Explicit native loopback overrides saved
TLS. LAN-only scopes existing authorized direct private peers and clears public
pools/allowlists before running the same Rust transport. It cannot enable WAN or
authorize discovered devices. Rings visualize activity; receipt verification is
separate. Same web package on mobile proves only shared Wasm/UI until the installed
mobile host can execute the package-owned native transport under its ordinary
permission policy. Do not silently move Dandelion into a mobile Host binary.
