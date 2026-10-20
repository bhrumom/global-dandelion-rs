# Native Apple Dandelion core

Status: implementation; 2026-10-20. Extends the canonical Rust architecture and
Fabushi Mini App contract; does not reduce the full cross-platform requirement.

## Problem and ownership

The signed Mini App can launch on Apple hosts but its native source composition
requires Linux openat2. Core scanning, transport, pacing, state and update code
already have suitable existing owners. Extend the existing platform adapter with
an Apple implementation; retain the package name for consumer/lock compatibility.
Do not create a second scanner or transport or substitute WebView filesystem access.

## Requirements and acceptance

- AP-01: Linux retains openat2 BENEATH/NO_SYMLINKS/NO_MAGICLINKS/NO_XDEV.
- AP-02: Apple source access holds a root directory and opens each relative
  normal component with openat/NOFOLLOW. Reject absolute paths, parent/current
  components, special files and device changes before reading. Intermediate
  directories remain held while resolving children. Never fall back to path open.
- AP-03: sample identity/content stamps from the opened handle. Reading must not
  change source data. Bound components, descriptors, bytes and blocking tasks.
- AP-04: existing real scan, loopback, mTLS send/receive, pacing, durable state,
  explicit import, repeat/watch, diagnostics and signed update paths run on macOS.
  Update verification uses the actual target triple, never a Linux hardcode.
- AP-05: platform discovery is method-specific. Linux mount identity/resume,
  proc/sys inventory and NetworkManager remain Linux-only pending native owners;
  unsupported operations fail explicitly before state reservation or OS mutation.
  This is an outstanding part of full parity, not a completed substitute.
- AP-06: exact-head Actions must run Linux and macOS native unit/integration/CLI
  checks, authenticated TLS and signed update fixture acceptance, and publish a
  runnable binary with source identity and checksum. Local tests/builds forbidden.
- AP-07: ship no claim of full Windows/mobile parity from this Apple work. iOS
  compilation, native Host integration, package provisioning, Windows adapters,
  real hotspot/hotplug and installed Host acceptance remain tracked requirements.

## Verification and delivery

Test nested symlink/parent/absolute escape, a renamed held root, file mutation,
special files, empty payload, bounded queue cancellation and negative credentials.
Use disposable local-only TLS peers, ephemeral certificates and signed update
fixtures in Actions. Do not transmit to public peers or use production credentials.
Preserve the existing full-replacement gates. A source binary artifact is a distinct
layer from the signed Mini App, Host installer and observed installed operation.

Promote through a reviewed PR after exact-head checks; versioned Host provisioning
must pin the resulting source/binary digest. Rollback restores the prior adapter
and host binary together. Do not overwrite a signed package in place.
