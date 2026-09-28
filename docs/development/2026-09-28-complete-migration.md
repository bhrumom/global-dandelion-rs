# Complete migration implementation record

Spec revision 2 replaces revision 1. Baseline: 7a0f78d; green run: 36421256894.
Requested outcome: all required legacy domains, not only the initial slice.

Current increment: durable state/import and platform inventory/watch, followed by
runtime integration and exact-revision CI. Verification records will identify actual
code and runs; unimplemented domains and hardware/performance gates remain open.

## Implemented production increment

- C15/C22: private-directory writer lock, checksummed versioned state, sync/rename/
  directory-sync commits, crash-conservative whole-run traffic leases, explicit
  idempotent import of the REAL Python traffic_state.json schema (current_month,
  total_sent_bytes). No monthly automatic reset, and byte/file resume remains open.
- C06/C07/C17: bounded mountinfo/proc/sys observation, passive TCP/ARP/interface
  reporting, mount identity differences and explicitly allowed mount namespaces.
  Runtime enrolls only mount roots within that policy. Hardware hotplug still needs
  real-device evidence; no new mount is an authorized network destination.
- C04/C05: real scan metadata/read commands; metadata never reads payload, read
  verifies selected file bytes through the pipeline without opening sockets.
- C13: day/night schedule with explicit UTC offset, positive validated rates and
  monotonic waits; deterministic boundary tests. No host time mutation.
- C19: latest-value terminal observer on an owned worker, 4 Hz refresh, terminal
  restoration guard, non-TTY rejection; headless output remains separate.
- C01: repeat/watch service conserves one budget across passes and cancellation;
  binary main reduced to parsing/log initialization/error mapping.

Tests and CI must be validated on the new exact revision before reporting success.

CI run 36426448387: dependency audit passed; native jobs stopped at a Clippy
collapsible-if lint in the new state adapter. Repairing the code without weakening
the lint gate; behavioral tests have not yet been claimed for this increment.

## Validation and authenticated peers

Run 36426655281 at aeb2baf passed both native architectures, dependency policy and
required gate for the durable/platform/scan/service increment. TLS extension now
uses rustls/tokio-rustls, exact authorized peers and mutual TLS 1.3. A meter below
TLS caps handshake and payload ciphertext across peers; a persistent token bucket
also spans repeated scans. Native receiver sends authenticated data and completion
receipts. CI generates ephemeral certificates for success, wrong name, untrusted
client and handshake-budget tests on loopback. New HEAD validation remains required.

Signed updater increment: Ed25519 exact-byte manifest verification, semantic-version/
architecture/expiry gates, bounded HTTPS or local input, SHA-256/size validation,
atomic versioned staging and explicit no-activation semantics. Shared WriteGuard
moved into core for all writers; state re-exports it without a concrete platform
dependency. New native offline CLI trust tests use ephemeral signing keys in CI.

Hotspot increment: plan/apply/status/stop through a whitelisted ProcessRunner port,
private password-file, durable UUID ownership journal, capability checks and scoped
rollback. CLI consent and fault-injected lifecycle tests added. Hardware gate open.

Dependency audit of the updater found the maintained public-root data's
CDLA-Permissive-2.0 license. Reviewed the published license and added a package-
specific exception only for webpki-root-certs, with original license text and
license inventory bundled into every artifact. No advisory/bans gate disabled.

File-level resume increment: bounded core checkpoint port, target/certificate/source
identity scoping, terminal receipt acknowledgement and persisted index. One-pass
`--resume` skips only unchanged acknowledged files, replays incomplete files whole,
and conserves durable traffic debt. Byte-level/exactly-once storage is not claimed.
