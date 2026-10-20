# macOS Mini App owns its native implementation

Status: active, 2026-10-20. Explicit user correction: do not put Mini App
functionality into the Host; these capabilities arrive by installing the Mini
App. Current priority is macOS. This supersedes Host-bundled engine composition.

## Requirements and design

- GD-MAC01: the signed package contains the shared Rust CLI, default config and
  its own tool router/CLI adapter. Fabushi ships only generic installation,
  account isolation, WebView, confirmation/picker and process lifecycle ports.
- GD-MAC02: retain fabapp v1 signed file table and managed trust root. Declare
  `native.runtime` in both package and reviewed catalog; minHostApi=2. The signed
  `native-runtime.json` resource binds platform darwin, architecture arm64,
  package worker entrypoint, native executable paths and broker permissions.
  Native code is an explicit privileged execution grant, never disguised text
  or a silently broadened offline signature. Web networking remains denied.
- GD-MAC03: package worker exclusively owns Dandelion methods, validated argv,
  picked-root checks, opaque IDs, bounded admission, cancellation and receipt
  parsing. Native dialogs return paths only to the native worker; Web receives
  bounded JSON/opaque IDs. Keep existing tests for this adapter, adding protocol
  and actual packaged-engine proof on Actions.
- GD-MAC04: one exact revision builds native CLI, adapter and WebAssembly/UI.
  All executable verification and packaging runs in Actions with disk reserve.
  No local builds/tests and no unpinned companion download.
- GD-MAC05: installing adds this code; close/account change removes active
  execution; uninstall removes archived code and transient extracted runtime.
  Reinstall and offline cold-launch use only approved local package bytes.
- GD-MAC06: preserve truthful native/platform capability detection. macOS
  inventory, external transfer onboarding and other remaining native parity
  work need real implementation/evidence before complete-Mac claims.

## Packaging, failure and acceptance

Offline 0.3.4 stays immutable; native macOS corrective version is 0.4.0.
Other platforms retain their prior package and are not advertised as native.
Unknown platform/descriptor/capability, bad signature/digest or denied native
confirmation fails closed. Native denial does not create a process. Existing
small UI limits and Rust operational safeguards remain unchanged.

GD-MAC01..05 require exact-head Actions tests, protected managed signing/review,
and real installed generic Host before completion. GD-MAC06 requires actual Mac
operations and truthful remaining evidence. All criteria currently pending.

Parents: `fabushi-miniapp.md`, native architecture, Desktop
`miniapp-owned-native-runtime.md`. This changes package ownership; it creates no
second protocol/engine/Registry or trust root.
