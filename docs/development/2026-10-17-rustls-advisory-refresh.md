# Dependency refresh: rustls advisory set of 2026-10-14

Date: 2026-10-17. Trigger: main push run 39043099921 failed the Dependency policy
job after the RustSec database loaded four advisories published 2026-10-14
against `rustls` 0.23.45 (RUSTSEC-2026-0346, -0347, -0348, -0349). The merge of
PR #2 was not the cause; the advisory database advanced after the previous green
run.

Resolution: `rustls` 0.23.45 -> 0.23.46 in `Cargo.lock` only, checksum
`48e13bd8c0e9365c43cfa5c9e8f9ad49d3c8444926c9aac819e0e4dc503c8fdf` from the
crates.io index. The 0.23.46 normal dependency set and requirements are identical
to 0.23.45, and the existing `rustls-webpki` 0.103.15 and `rustls-pki-types`
1.15.1 already satisfy `^0.103.14` and `^1.12`, so no other lock entry changes.

No source, feature or manifest change. Local `cargo` is unavailable on this
workstation, so the lockfile edit is validated only by Actions
(`cargo deny --locked`, `cargo audit` and the locked builds); it is not claimed
green until the exact revision passes.
