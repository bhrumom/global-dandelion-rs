# Global Dandelion RS

[Chinese documentation](README.zh-CN.md)

A native Linux Rust CLI replacing the Python program through explicit, bounded,
testable components. **Spec revision 2.1 replaces the original first-slice Spec.**
This branch now contains real implementations across transfer, state/resume,
platform discovery, pacing, TUI, hotspot, signed updates and legacy compatibility.
Full hardware and controlled whole-program cutover acceptance remains separate from
code completion and automated CI; see [acceptance gates](docs/acceptance-gates.json).

## Native command line

```sh
dandelion --help
dandelion self-test --json
dandelion config init
dandelion --config examples/service.toml config check
dandelion run --source /srv/data --dry-run --json
dandelion run --source /srv/data --repeat 3 --interval-ms 1000 --json
dandelion --config examples/service.toml run --resume --json
dandelion run --source /srv/data --tui
dandelion scan metadata --source /srv/data --json
dandelion scan read --source /srv/data --json
dandelion platform inspect
dandelion platform watch --samples 10 --interval-ms 1000
dandelion platform time
dandelion state inspect --directory .dandelion-state
dandelion state import --directory .dandelion-state --file traffic_state.json
dandelion completions bash
```

`run` defaults to one pass and local UDP verification. `--watch` is explicit and
still constrained by session/lifetime budgets; repeat passes share the same token
bucket, so repeating a scan cannot manufacture new burst allowance. `--dry-run`
validates configuration without sockets/source scanning. `scan metadata` never
reads payload; `scan read` runs paced source verification without sockets.

## Architecture: 13 implemented workspace packages

```text
CLI -> configuration -> supervised service
                   -> source/checkpoint/process/storage capability ports
source -> blocking reader -> bounded queue -> versioned packet -> limiter/budget
                                                       -> transport -> receipt
receipts -> checkpoints / immutable telemetry -> latest-value TUI
```

| Package | Production responsibility |
|---|---|
| app | commands, composition, service lifecycle, signals, output |
| core | domain values, typed errors, capability ports, target and rate policies |
| config | validated defaults/TOML/environment/CLI precedence |
| scanner | bounded read-only traversal, metadata/read modes and checkpoint skip |
| packet | documented v2 framing, checked decode, BLAKE3 checksum |
| pacing | monotonic integer token bucket and explicit day/night schedule |
| transport | local UDP, mutual TLS peers, receiver, actual socket-byte metering |
| platform-linux | rooted file access, mount/interface observation, privileged adapters |
| state | exclusive writer, atomic durable ledger, conservative leases and file resume |
| tui | independent latest-value terminal observer with restoration guard |
| updater | Ed25519 trust verification, semantic versions, bounded artifact staging |
| hotspot | explicit NetworkManager lifecycle and journaled owned-profile rollback |
| compat | isolated legacy MD5 framing and country CSV metadata |

Core has no Tokio, Linux, CLI or socket-implementation dependency. App injects
SourceAccess/SourceRoot/SourceFile, Checkpoints, ProcessRunner and WriteGuard.
Scanner never imports concrete Linux/state adapters. No empty feature packages,
mutable global application maps, unbounded production queues or application unsafe
Rust. CI checks dependency direction; it is not a substitute for runtime tests.

## Authenticated LAN/private/VPS/WAN peers

Local UDP remains the default. External transfers use **mutual TLS 1.3**, trusted
CA/client/server identities, explicit numeric targets and verified server names.
TLS names do not perform destination DNS expansion. A public peer requires WAN enable and authorization by the effective target policy.
Revision 2.1 bounded public target pools are implemented through a TargetPool plus
separate sequential, randomized and deterministic-seeded TargetSelector. Pools are
explicitly configured/authorized CIDRs, selection is bounded, and every generated
address is revalidated by the effective target policy before authenticated session
establishment. The selected concrete peers are visible in runtime target health.
Discovery never authorizes a network peer. See [TLS example](examples/tls-client.toml).

```sh
dandelion receive --bind 127.0.0.1:20000 --ca-file credentials/ca.pem \
  --certificate-file credentials/server.pem --private-key-file credentials/server.key --json
dandelion --config examples/tls-client.toml run --json
dandelion --config examples/tls-client.toml platform time --peer
```

Private keys must be regular private-permission files, never inline arguments.
Client credential files are excluded from scanning. A non-loopback receiver also
requires explicit `--allow-remote` and `--allowed-client` values. Do not expose it
with test keys or an unreviewed trust policy. Receipts mean authenticated in-memory
verification, **not durable remote file storage**. Frames are not silently retried
after an ambiguous receipt. Connection retries are bounded before payload sending.

The meter is **below TLS**: handshake, encrypted records, acknowledgements and
control messages count. Counters measure bytes accepted by the socket API, not
IP/TCP headers, kernel retransmissions or physical-link traffic. Per-peer verified
packets/payload and actual ciphertext counters are included in final snapshots.

## State, resume and source safety

State is optional and off by default. Enabling it requires a private directory;
a single writer lock prevents competing jobs. Commits sync a temporary file,
atomically replace state, and sync the directory. Before network startup, a
whole-run budget lease is durably charged. Clean settlement returns proven unused
bytes; failures/crashes conservatively retain the lease. Corrupt/future state never
silently resets traffic debt. This is a lifetime budget, not an automatic monthly reset.

`--resume` requires enabled state and one pass. It stores up to 4,096 acknowledged
file identities, scoped to target policy and TLS trust/client certificate. Unchanged
acknowledged files are skipped; changed/incomplete files are read again. Checkpoints
commit at controlled settlement; SIGKILL/host crash can replay work since the last
commit. **This is file-level, at-least-once resume, not byte-level or exactly-once storage.**

Source opens use Linux openat2 beneath selected roots, with no symlink/magic-link
escape or mount crossing. File-handle metadata detects common changes, not arbitrary
same-user adversaries or filesystem snapshots. The normal transfer/read modes never
rename or modify source contents. Explicit compatibility link exercises necessarily
change inode metadata and require a quiescent trusted source directory.

## Optional operational capabilities

`platform inspect/watch` passively observes mounts, interface counters, TCP and
IPv4 ARP. Automatic mount enrollment is off unless explicit non-root mount prefixes
are configured. Mid-read unmounts fail closed; do not assume arbitrary hotplug fault
recovery or driver compatibility without real hardware evidence.

`--tui` renders snapshots on stderr without blocking the data path, including live
monotonic rates. Non-TTY use remains headless. Real PTY acceptance tests verify
alternate-screen restoration under signals. `platform time --peer` estimates an
authenticated peer's clock offset; it never changes host time or asserts UTC authority.

`hotspot plan` is read-only; `apply/stop` require `--confirm`, existing system
capability and a private journal. Only an owned temporary NetworkManager profile
may be changed/removed; rollback respects later operator changes. No hidden sudo,
password command arguments, autoconnect or radio-regulation overrides. Real wireless
hardware acceptance remains outstanding. See [operations](docs/operations.md).

`update check/stage` verifies an independently trusted Ed25519 public key, manifest
signature, version, architecture, expiry, SHA-256 and exact artifact size. Explicit
HTTPS origins and no redirects are enforced. Staging never executes/replaces the
active binary. [systemd templates](packaging/systemd/) provide the supported headless service and
periodic signed release checking. The service is designed for `systemctl enable`
boot startup, systemd-owned restart supervision, journald-compatible logging and
graceful SIGTERM shutdown; installation still does not silently enable transmission.
Production key provisioning and activation/rollback require operator validation.

`compat` contains bounded country lookup, legacy packet inspection, and opt-in
journaled link exercise/recovery. Unrestricted public-Internet spray remains
excluded. Authorized-pool target selection is implemented and covered by core/config/
shipping-composition tests; CI never sends workload data to those public targets.
MD5 is legacy interoperability, not authentication.

## Build, test, artifacts and limitations

Build/test/package on disposable GitHub Actions runners, not storage-constrained
persistent devices. Pin Rust 1.98.1, edition 2024 and Cargo.lock. Linux x86_64/ARM64
artifacts are built on Ubuntu 24.04 GNU/glibc, require compatible userspace and
Linux 5.6+ safe-open support, and are not macOS/static-musl packages.

Every mandatory automated gate runs formatting, Clippy, unit/property/CLI tests,
real UDP/mTLS negative tests, state/resume, PTY restoration, signed update staging,
architecture/parity checks, RustSec and licensing policy. Archives preserve executable
mode, checksums, build metadata, original third-party license notices, acceptance
JSON and raw benchmark evidence. Inspect the **exact revision and successful run**.
These are CI artifacts, not a production signed release.

```sh
sha256sum --check dandelion-x86_64-unknown-linux-gnu.tar.gz.sha256
mkdir dandelion-bin
tar -xzf dandelion-x86_64-unknown-linux-gnu.tar.gz -C dandelion-bin
cd dandelion-bin
sha256sum --check SHA256SUMS
./dandelion self-test --json
```

Before all writes/downloads/builds preserve max(1 GiB, 5% filesystem capacity).
Queued memory estimates are not total RSS. Source-read, traffic, receiver-session,
metadata, checkpoint and update sizes all have independent explicit limits.
Shared-runner packet benchmarks compare the exact reviewed pure Python framing
expression to Rust; they do not prove whole-program or WAN throughput improvement.
Full physical hotspot/hotplug, production release trust and representative controlled
cutover measurements remain separate acceptance gates, never implied by a green build.

## Source of truth

[Spec v2.1](docs/specs/global-dandelion-rust-architecture.md) replaces the archived v1.
[Migration ledger](docs/migration/python-parity-ledger.md), [ADRs](docs/adr/),
[development record](docs/development/2026-09-28-complete-migration.md),
[operations](docs/operations.md), [benchmark methodology](docs/benchmarks.md).
Python source pinned to `bhrum/global@aa7f3720c42856f0e1d18742f78fba156227ee79`,
blob `54bf6226eb80d1ef6c4f6087fb02579141ab134f`. The owner selected proprietary
distribution on 2026-10-13; see [LICENSE](LICENSE). Repository access does not
grant source reuse rights. Cargo workspace publishing remains disabled.

## Fabushi offline mini app

The Desktop Web/Wasm packet workbench under `miniapp/` reuses the same GDAN v2
encoder/decoder from this native implementation. Explicit file selection,
bounded local packet verification, cancel and report export need no network.
This interface does not perform native LAN/WAN transfer, hotspot changes or
system scanning. Those functions retain their native owners and require a
reviewed Fabushi Host adaptation before mini-app parity can be claimed.

See [mini-app spec](docs/specs/fabushi-miniapp.md) and the Marketplace draft.
Unsigned CI preparation and ephemeral test-signing fixtures are not trusted
production packages or evidence of public publication. Managed signing,
publisher/scan/review and packaged Fabushi offline acceptance are release gates.
