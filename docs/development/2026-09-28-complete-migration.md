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

Legacy compatibility increment implements bounded country lookup, exact old packet
inspection, and explicit private-workspace link exercise/recovery. Source inspection
corrected the old effect description: links are renamed, not the original filename.
The unsafe filename-pattern deletion heuristic is not restored.

Acceptance additions: actual isolated PTY terminal entry/restore under SIGTERM and
an equivalent pure Python/Rust packet benchmark. Neither changes user terminal,
network configuration or production files. Both run only on disposable CI runners.

CI run 36430585509 exposed a permissions mismatch in compatibility recovery:
tempfile directories default to umask-filtered 0777, while recovery correctly
requires a private directory. Set 0700 explicitly at creation for link workspaces
and update staging bundles; keep the strict recovery check, and improve test error
reporting. No existing user directory permissions are changed.

Clock/telemetry increment: local metadata and optional authenticated peer clock
comparison, metered and durable-budgeted, with no host-clock mutation or UTC accuracy
claim. Added monotonic live rate estimation and actual mTLS clock-fixture acceptance.

Per-target health now includes authenticated peer identity, verified packet/payload
counts and actual ciphertext accepted on that peer's metered socket. Successful
repeat sessions aggregate these values; failures log the available target snapshot
instead of pretending completion. Backward-compatible empty fields preserve legacy
state checksums. Real TLS acceptance checks target/socket totals agree.

Integration record: current production implementation at c9b8cade84cd passed run
36431323900, including native TLS/clock/target counters and prior acceptance suites.
Documentation, examples and migration inventory now replace first-slice descriptions.
The 37 rows distinguish 34 code implementations and 3 deliberate unsafe-default
exclusions from physical hardware, production trust and full cutover acceptance.
The complete gate requires external reviewed exact-revision evidence and cannot be
satisfied merely by a zero-pending code list. New docs/config/CLI changes still
require a fresh current-revision CI run.


## Resume scope v2 and evidence-contract closure

A post-migration audit found two claims that were not yet true in the branch: the
Spec still named a nonexistent dandelion-telemetry crate, and production resume
still used GDAN/file-resume/v1 without Linux mount identity. The full-replacement
checker also trusted manifest fields without re-reading and hashing the gate report.

This change closes those gaps without claiming external acceptance. Telemetry/rate
models remain in dandelion-core and dandelion-tui remains an observer; the static
architecture gate now compares the Spec crate block with the actual workspace.
C16 freezes allowed-mount discovery for a finite resume pass and hashes canonical
source roots plus the exact visible Linux mount generation into
GDAN/file-resume/v2. Unit tests prove remount/device replacement changes identity.

The external evidence contract is now machine-checked: exact commit, exact gate ID,
bounded regular report file, SHA-256, pass result and reviewer independence from the
report performer. CI includes synthetic positive and tamper/self-review/missing-
report/wrong-gate negative tests. These synthetic tests verify the checker only and
are never accepted as real hotspot/hotplug/cutover/release-trust evidence.

The commit containing this record still requires exact-revision CI. Physical Wi-Fi
hotspot, physical hotplug, controlled cutover and production release trust remain
external gates and must not be marked passed without independently reviewed reports.


## Exact-head artifact provenance repair

A real acceptance-host inspection of workflow run 36452638250 found that GitHub's
pull-request run metadata reported head_sha
e78a9bd506726450b5f53c009f4e269acf4a75c5 while the extracted aarch64 artifact's
checksummed BUILD-INFO.txt recorded git_sha
da5837c4d524b7120eda8b80982bc1c547fde23a, the synthetic pull-request merge commit.
Therefore that otherwise-green run and its artifacts are not accepted as exact-head
evidence for e78a9bd. No controlled-cutover pass is claimed from them.

The CI workflow now derives TESTED_COMMIT from pull_request.head.sha for pull-request
runs (or github.sha otherwise), explicitly checks out that revision in every job,
verifies git rev-parse HEAD before testing, records TESTED_COMMIT in BUILD-INFO.txt,
and names artifacts with the same commit. The controlled-cutover harness additionally
requires the extracted artifact BUILD-INFO.txt and rejects any git_sha that differs
from the tested commit, so workflow metadata alone cannot mis-bind a binary.

On acceptance host bhrum2, the negative provenance check was exercised against the
old artifact and correctly failed with the synthetic-merge BUILD-INFO SHA mismatch.
The next exact-head CI run and artifact must complete successfully before current-ci
or controlled-cutover candidate evidence can advance.


## Gate-specific evidence semantic closure

A continuation audit found another fail-open path in full-replacement acceptance:
check_parity.py authenticated report bytes, commit/gate binding and distinct reviewer
names, but a correctly hashed minimal report could still say result=pass without
containing the physical or production facts required by that gate. Reviewer metadata
was also not bound to the exact report digest and commit.

The verifier now rejects generic pass envelopes. Each of the five gates has explicit
semantic validation matching the Spec. current-ci requires both native artifacts and
exact BUILD-INFO/checksum/self-test provenance; physical-hotspot requires real
non-virtual Wi-Fi identity plus apply/interruption/rollback; physical-hotplug requires
distinct real original/replacement device identities and same-path checkpoint
invalidation/recovery; controlled-cutover requires raw repeated same-fixture samples
and complete Python/Rust transfer integrity; production-release-trust requires a
non-synthetic production signing identity/origin plus activation and rollback health.
Reviewer entries now bind approve to the exact report digest and tested commit and
must provide an immutable external review reference.

The evidence self-test now has a negative case that recomputes a valid digest around a
semantically empty hotspot report and confirms it still fails. controlled_cutover.py
also now fails if the legacy UDP receiver loses packets/payload or wire-byte accounting
does not match, rather than treating only MD5 corruption as failure.

Acceptance-host inventory was rechecked on bhrum2 while making this change: only
enp0s6 plus loopback/Docker interfaces are present, NetworkManager nmcli is absent,
and the sole physical block device is the non-hotplug/non-removable system disk sda.
Therefore neither physical-hotspot nor physical-hotplug is promoted from that host.
This change invalidates prior exact-HEAD candidate evidence for completion purposes;
fresh reports must bind the new commit after its CI succeeds.
