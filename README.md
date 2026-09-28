# Global Dandelion RS

[Chinese documentation / README.zh-CN](README.zh-CN.md)

A Linux-first, unprivileged Rust CLI rearchitecture of `global_dandelion.py`.
The executable is **`dandelion`**, not a Python wrapper.

**Current scope: the first working vertical slice, not full Python feature parity.**
It scans explicitly selected directories once, frames chunks, applies bounded
backpressure and pacing, sends real UDP on loopback, and verifies received bytes.
WAN/LAN/VPS, automatic drive discovery, hotspot, TUI, persistent resume and
self-update are not implemented in this slice. Unsupported modes fail validation.
No speedup over Python is claimed without the controlled baseline comparison.

## Quick start

Requirements: Linux kernel 5.6+ (`openat2`), an x86_64 or aarch64 host, and Rust
1.98.1 when building from source. No root privileges are required.

Prefer the binary artifacts from a **successful** `Rust CLI Quality` Actions run.
Each includes a platform-specific `.tar.gz` and its `.sha256` file.
Verify the outer archive first, extract it, then verify `SHA256SUMS` inside.
The tar archive preserves the executable permission of `dandelion`; it also
contains `Cargo.lock`, `BUILD-INFO.txt` and the acceptance result.
For example, after downloading and unpacking the GitHub artifact ZIP:

```sh
sha256sum --check dandelion-x86_64-unknown-linux-gnu.tar.gz.sha256
mkdir dandelion-bin
tar -xzf dandelion-x86_64-unknown-linux-gnu.tar.gz -C dandelion-bin
cd dandelion-bin
sha256sum --check SHA256SUMS
./dandelion self-test --json
```
Artifacts are CI builds, not a signed public release or a guarantee of bit-for-bit reproducibility.

```sh
dandelion --help
dandelion doctor
dandelion self-test --json
dandelion config init
dandelion --config examples/loopback.toml config check
dandelion --config examples/loopback.toml run --json
dandelion run --source /path/to/your/data --dry-run --json
dandelion run --source /path/to/your/data --traffic-limit-bytes 67108864 --json
dandelion completions bash
```

`config init` prints to stdout; it does not overwrite a file. Inspect available
storage before redirecting output. `run` is a finite one-pass job, not an infinite
background scanner. `Ctrl+C` and `SIGTERM` cancel the job and return exit code 130.
Files are **read only** and are not renamed, rewritten, or persisted by the receiver.
The loopback receiver proves packet receipt/integrity, not remote file storage.

### Build and verify

Use GitHub Actions for all compilation, tests, security auditing and release builds.
Do not run heavy builds on persistent devices with insufficient disk reserves.
For an explicitly provisioned disposable development machine:

```sh
python3 scripts/disk_guard.py --planned-bytes 6442450944
cargo build --locked --release -p dandelion-app
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
python3 scripts/acceptance.py --binary target/release/dandelion
```

## Architecture

```text
CLI / configuration
       |
   Supervisor -- cancellation, single-owner budget, typed outcomes
       |
   blocking scanner -> bounded mpsc -> packet v2 -> token bucket
                                                  |
                                        transport port (trait)
                                                  |
                                      connected UDP loopback pair
                                                  |
                                       decode + exact-byte verifier
```

| Package | Responsibility |
| --- | --- |
| `dandelion-app` | Commands, composition, task supervision, signals, logging |
| `dandelion-core` | Domain values, typed failures, immutable snapshots, budget |
| `dandelion-config` | Defaults/TOML/environment/CLI precedence and validation |
| `dandelion-scanner` | One bounded, cancellable blocking file pipeline |
| `dandelion-packet` | Versioned framing, zero-copy decode, BLAKE3 checksum |
| `dandelion-pacing` | Integer monotonic-clock token bucket |
| `dandelion-transport` | Transport interface and real local UDP verifier |
| `dandelion-platform-linux` | Kernel-bounded source opens and disk reserve checks |

The core has no Tokio, CLI, Linux or socket dependency. Filesystem access is
inverted through `SourceAccess -> SourceRoot -> SourceFile` capability ports;
the application injects the Linux implementation. The scanner does not depend
on the concrete platform package in production. Only the Linux adapter knows
`openat2`, `statvfs` and device/inode timestamp layouts. No empty placeholder
packages are created for unimplemented domains.

## Safety and limits

Defaults: 32 queued chunks, 1,152-byte payloads, 1 MiB/s wire-rate target,
1,232-byte burst, 64 MiB wire budget, 256 MiB source-read budget, depth 64,
one million files, and a 3-second cooperative shutdown deadline.
The memory setting bounds queued packet payload estimates, **not total RSS**;
Tokio, traversal metadata, file descriptors and OS socket buffers are separate.

Only `127.0.0.1` sockets are opened. No public-IP generation, LAN discovery,
background updating, system clock mutation, shell execution or privileged network
configuration is performed. BLAKE3 is an unkeyed checksum, not authentication or encryption.
Sources should be trusted data directories. Hidden names and common generated
folders are excluded by default; symbolic links and mount crossings are rejected.
Hard links, overlapping bind mounts and an attacker who controls source contents
are not a confidentiality boundary. Runtime metadata checks detect ordinary
mutations but do not provide a filesystem snapshot.

The scanner holds at most `max_depth + 1` directory handles to avoid WalkDir's
collect-to-close behavior. It neither sorts nor retains the complete file tree.
A stalled consumer backpressures the producer. On cancellation/error, the
receiver is dropped before joining the producer. A kernel-blocked filesystem
syscall cannot be forcibly interrupted by aborting a Rust task: timeout is an
explicit error, and the CLI does not wait indefinitely while dropping its runtime.

## Documentation

- [Authoritative architecture Spec](docs/specs/global-dandelion-rust-architecture.md)
- [First-slice decisions and boundaries](docs/adr/0001-first-vertical-slice.md)
- [Packet v2 byte layout](docs/protocol-v2.md)
- [Configuration and operations](docs/operations.md)
- [Python migration ledger](docs/migration/python-parity-ledger.md)
- [Benchmark methodology](docs/benchmarks.md)

Pinned source: `bhrum/global@aa7f3720c42856f0e1d18742f78fba156227ee79`,
`global_dandelion.py` blob `54bf6226eb80d1ef6c4f6087fb02579141ab134f` (3,179 lines).
No source license is inferred from repository access; workspace publishing is disabled.
