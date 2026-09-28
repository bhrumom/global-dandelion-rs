# Global Dandelion Rust Rearchitecture Specification

Status: **Draft / architecture baseline**
Repository: `bhrumom/global-dandelion-rs`
Source baseline: `bhrum/global@aa7f3720c42856f0e1d18742f78fba156227ee79`
Primary source: `global_dandelion.py`
Source blob SHA: `54bf6226eb80d1ef6c4f6087fb02579141ab134f`

## 1. Purpose

This repository is a ground-up Rust rearchitecture of the existing Python Global Dandelion program.

The goal is **not** a line-by-line translation. The goal is to preserve the legitimate product behavior that matters while replacing the current Python thread/queue/global-state design with a bounded, observable, testable, crash-safe, Linux-native Rust architecture.

The implementation MUST optimize for:

1. predictable resource usage;
2. high-throughput local and explicitly authorized network transfer;
3. safe cancellation and shutdown;
4. clear ownership of state and concurrency;
5. minimal privileged surface area;
6. deterministic configuration;
7. inspectable metrics and logs;
8. testability without requiring public-network traffic;
9. migration compatibility where it is actually needed;
10. long-term maintainability over micro-optimizations.

## 2. Current-system baseline

The Python baseline is approximately 3,180 lines in `global_dandelion.py` and currently mixes orchestration, Linux platform access, scanning, packet construction, networking, pacing, status display, updating, hotspot control, and persistence in one process.

Observed characteristics of the baseline include:

- 17 `threading.Thread` subclasses/usages;
- 3 `queue.Queue` instances;
- multiple `Lock`/`RLock` protected global structures;
- roughly 10 socket creation sites;
- multiple UDP `sendto()` hot paths;
- repeated `os.walk()` directory traversal;
- per-chunk MD5 calculation and packet allocation;
- direct `/proc` and `/sys` parsing;
- numerous `subprocess` calls;
- WAN, LAN, loopback, VPS, hotspot, drive monitoring, pacing, TUI/status, updater, and state logic in one module;
- worker scaling based on `CPU_COUNT * 50`;
- unbounded loopback queue behavior;
- repeated per-packet/per-worker status updates.

These are architecture smells to remove, not patterns to reproduce.

## 3. Scope

### 3.1 In scope

The Rust system will provide professionally designed equivalents for the legitimate portions of the existing program:

- source-directory discovery;
- external-drive monitoring;
- file walking and filtering;
- file chunking;
- transfer framing and integrity checks;
- loopback transfer;
- LAN transfer to configured local targets;
- explicitly configured private/VPS targets;
- explicitly authorized WAN targets;
- pacing and bandwidth policies;
- global traffic accounting;
- status and telemetry;
- headless operation;
- terminal UI;
- Linux network/disk inspection;
- hotspot lifecycle where supported;
- persistent runtime state;
- controlled self-update/release checking;
- graceful shutdown;
- compatibility adapters required during migration.

### 3.2 Explicitly not reproduced as a default

The Rust rewrite MUST NOT make random public-IP spraying or broad unsolicited Internet transmission a default behavior.

WAN transmission MUST require an explicit configured destination set or allowlist. Public-network targets MUST be opt-in, visible in configuration/status, rate-limited, and auditable.

CI and automated tests MUST NOT send traffic to arbitrary public hosts.

### 3.3 Out of scope for the first implementation

- GUI desktop application;
- Windows/macOS platform parity;
- distributed control plane;
- cloud-managed fleet orchestration;
- kernel modules;
- custom network drivers;
- stealth/evasion behavior.

Linux is the first-class target.

## 4. Design principles

### 4.1 Architecture before optimization

No optimization is accepted unless it can be measured with a benchmark or production metric.

### 4.2 Bounded concurrency

Every producer/consumer edge MUST have one of:

- a bounded channel;
- a bounded semaphore;
- explicit backpressure;
- a documented loss/drop policy.

Unbounded queues are prohibited in production hot paths.

### 4.3 No mutable global application state

Application state MUST be owned by components and shared via typed handles/channels.

Global mutable maps protected by ad-hoc locks are prohibited.

### 4.4 Least privilege

The main process SHOULD run unprivileged.

Operations requiring `CAP_NET_ADMIN`, root, hotspot configuration, clock modification, or device management MUST be isolated behind explicit platform adapters and MUST fail closed when unavailable.

### 4.5 Monotonic time for runtime decisions

Rate control, retries, deadlines, speed measurements, and watchdogs MUST use monotonic clocks.

Wall-clock/network time is metadata only and MUST NOT be required for correct runtime scheduling.

### 4.6 Explicit compatibility

Legacy behavior is preserved only through named compatibility modules/features. Compatibility code MUST NOT silently become the core architecture.

## 5. Proposed repository structure

```text
global-dandelion-rs/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── README.md
├── AGENTS.md
├── docs/
│   ├── specs/
│   │   └── global-dandelion-rust-architecture.md
│   ├── migration/
│   │   └── python-parity-ledger.md
│   └── adr/
├── crates/
│   ├── dandelion-app/
│   ├── dandelion-core/
│   ├── dandelion-config/
│   ├── dandelion-scanner/
│   ├── dandelion-packet/
│   ├── dandelion-transport/
│   ├── dandelion-pacing/
│   ├── dandelion-platform-linux/
│   ├── dandelion-hotspot/
│   ├── dandelion-state/
│   ├── dandelion-telemetry/
│   ├── dandelion-updater/
│   └── dandelion-tui/
├── tests/
│   ├── integration/
│   ├── fixtures/
│   └── parity/
├── benches/
└── .github/
    └── workflows/
```

The workspace MAY be reduced if early implementation proves some crates are too small to justify independent packages, but dependency direction MUST remain clear.

## 6. Dependency direction

The dependency graph MUST be mostly inward:

```text
dandelion-app
  ├─ config
  ├─ core
  ├─ scanner
  ├─ transport
  ├─ pacing
  ├─ platform-linux
  ├─ hotspot
  ├─ state
  ├─ telemetry
  ├─ updater
  └─ tui

transport -> packet, core
scanner   -> core
pacing    -> core
platform  -> core
tui       -> core
```

Core MUST NOT depend on Linux implementation details, terminal rendering, updater logic, or concrete socket implementations.

## 7. Runtime architecture

### 7.1 Runtime

Use Tokio as the orchestration runtime.

Tokio is used for:

- cancellation;
- timers;
- bounded channels;
- async socket coordination;
- status fan-out;
- task supervision.

Blocking filesystem traversal and regular-file reads MUST not run on Tokio core executor threads. They MUST use dedicated blocking workers or a dedicated scanner/read pool.

### 7.2 Supervisor

The application has one top-level `Supervisor`.

Responsibilities:

- load validated configuration;
- initialize telemetry;
- start components in dependency order;
- own the global cancellation token;
- supervise task exits;
- apply restart policy where appropriate;
- initiate graceful shutdown;
- flush state and telemetry;
- return deterministic process exit codes.

No component may call `std::process::exit` directly.

### 7.3 Cancellation

Use a shared hierarchical cancellation mechanism such as `tokio_util::sync::CancellationToken`.

Every long-running loop MUST have a cancellation path.

Shutdown order:

1. stop accepting new scan work;
2. stop producing new chunks;
3. stop enqueueing transfer jobs;
4. drain or cancel transport according to configured shutdown deadline;
5. persist state;
6. stop UI/telemetry;
7. exit.

## 8. Core domain model

The core crate defines domain types only.

Suggested types:

- `SourceRoot`
- `FileIdentity`
- `FileMetadata`
- `ChunkIndex`
- `ChunkPayload`
- `TransferId`
- `TransferTarget`
- `TargetSet`
- `TransportMode`
- `RatePolicy`
- `TrafficBudget`
- `TransferProgress`
- `ComponentHealth`
- `RuntimeSnapshot`

All identifiers crossing task boundaries MUST be owned values or immutable shared values.

Path handling MUST use `PathBuf`/OS strings internally rather than assuming UTF-8.

## 9. Configuration

Configuration precedence:

1. compiled defaults;
2. config file;
3. environment overrides;
4. CLI overrides.

Preferred config format: TOML.

Example top-level sections:

```toml
[runtime]
shutdown_timeout_seconds = 15

[scanner]
roots = []
exclude_hidden = true

[transport.loopback]
enabled = true

[transport.lan]
enabled = false
targets = []

[transport.vps]
enabled = false
targets = []

[transport.wan]
enabled = false
allowed_targets = []

[pacing]
profile = "day"

[traffic]
global_limit_bytes = 0

[hotspot]
enabled = false

[update]
enabled = true
channel = "stable"
```

Config MUST be schema validated before component startup.

Invalid IP addresses, port ranges, rate profiles, or overlapping contradictory settings MUST produce actionable errors.

Secrets MUST NOT be written to logs.

## 10. Scanner and file pipeline

### 10.1 Scanning

The scanner MUST:

- walk configured roots;
- skip configured exclusions;
- avoid hidden/system paths by policy;
- avoid duplicate roots;
- detect disappeared/unmounted roots cleanly;
- emit immutable file jobs into a bounded channel;
- avoid rescanning the entire tree concurrently from every sender.

A single logical scan result SHOULD fan out to interested transfer pipelines.

### 10.2 File identity

A stable file identity SHOULD use:

- device id + inode where available;
- canonical metadata snapshot;
- path as display/context, not sole identity.

### 10.3 File reads

Use sequential reads with a configurable chunk size.

The initial chunk size SHOULD be significantly larger than the Python WAN default of 1 KiB, but the final value MUST be selected from benchmark results and protocol constraints.

Small-file and large-file behavior MUST be benchmarked separately.

### 10.4 Renaming effect

Any rename behavior is optional and disabled by default.

If retained for compatibility, it MUST:

- use an explicit journal;
- restore original names after interruption where possible;
- never rename the repository/config/state files;
- avoid races with open file handles;
- be isolated in a compatibility component.

## 11. Packet protocol

### 11.1 Versioned protocol

Define a versioned packet envelope instead of an implicit `chunk_index + md5 + payload` layout.

Proposed logical fields:

- magic;
- protocol version;
- flags;
- transfer id;
- file id;
- chunk index;
- payload length;
- integrity tag;
- payload.

Encoding MUST have a stable documented byte order.

### 11.2 Integrity

The new protocol SHOULD use a fast modern integrity primitive such as BLAKE3 or CRC32C depending on benchmark and threat model.

Legacy MD5 framing MAY exist behind a `legacy-v1` compatibility feature if required for migration. It MUST NOT be described as cryptographic integrity.

### 11.3 Allocation policy

Packet encoding SHOULD reuse buffers where safe.

Avoid allocating a new vector for every destination when one encoded chunk can be shared immutably across sends.

## 12. Transport architecture

Define a transport trait/interface around transfer intent rather than raw sockets.

Conceptually:

```text
Scanner -> Chunk Producer -> Transfer Job Queue -> Transport -> Target
```

Transport implementations:

- Loopback transport;
- LAN transport;
- configured VPS/private transport;
- authorized WAN transport;
- legacy transport adapter if needed.

### 12.1 Target authorization

Every non-loopback target MUST come from validated configuration or an explicitly approved discovery source.

The transport layer MUST reject destinations outside the effective target policy.

### 12.2 UDP socket ownership

Sockets MUST have clear single-owner or shared-safe semantics.

Do not pass one global mutable socket through unrelated components.

### 12.3 Batching

On Linux, transport MAY use safe wrappers around batching primitives such as `sendmmsg` after correctness is established.

Batching is an optimization, not a semantic requirement.

Batch size MUST be configurable and benchmarked.

### 12.4 Backpressure

When the network cannot keep up:

- scanner/file readers MUST eventually slow down;
- memory usage MUST remain bounded;
- state/UI MUST expose the backlog;
- queue overflow MUST never silently allocate without bound.

## 13. Loopback transport

Loopback is the primary performance and correctness test transport.

Requirements:

- destination range is explicit;
- no public network dependency;
- deterministic benchmark mode;
- receive-side verification test fixture;
- packet loss/corruption counters where receive verification is active.

The Rust implementation MUST not duplicate full filesystem traversal once per sender thread as the Python implementation currently does.

## 14. LAN/private/VPS transport

LAN and private/VPS targets MUST be explicit.

Discovery, if enabled, MUST be a separate component from sending.

ARP/neighbour inspection is observational; discovered devices do not automatically become transfer destinations unless policy permits it.

## 15. WAN transport

WAN is disabled by default.

To enable it, configuration MUST provide authorized targets.

Requirements:

- allowlist validation;
- visible effective target set;
- configurable rate limit;
- configurable global traffic budget;
- retry/backoff policy;
- error classification;
- target health accounting;
- no random whole-Internet fallback.

## 16. Pacing and traffic accounting

Pacing is a first-class component, not sleeps scattered through loops.

A rate controller owns:

- bytes-per-second target;
- burst allowance;
- concurrency allowance;
- day/night profile;
- constant-speed profile;
- global traffic budget.

Preferred algorithm: token bucket or equivalent monotonic-time limiter.

Traffic counters SHOULD distinguish:

- attempted bytes;
- accepted-by-kernel bytes;
- verified receive bytes where available;
- per-transport totals;
- per-target totals.

State updates SHOULD be aggregated rather than locking on every packet.

## 17. Linux platform layer

Linux-specific functions live in `dandelion-platform-linux`.

Responsibilities may include:

- mounts;
- filesystem usage;
- network interfaces;
- connection summaries;
- removable-drive observation;
- neighbour table inspection;
- network configuration helpers.

Prefer native interfaces and maintained crates over spawning commands.

Possible technologies, subject to implementation review:

- `procfs`;
- `nix`;
- `rustix`;
- `rtnetlink`;
- `netlink-packet-*`;
- `udev` bindings where justified.

Any remaining subprocess use MUST be isolated, timeout-bounded, argument-safe, and observable.

## 18. Hotspot subsystem

Hotspot control is privileged and optional.

It MUST be isolated behind a capability boundary.

The main transfer engine MUST function without hotspot support.

Initial implementation MAY coordinate existing Linux tools such as NetworkManager/hostapd/iw through a narrow adapter if a reliable native alternative is not practical.

The adapter MUST:

- validate interface names and channel settings;
- never build shell command strings from untrusted text;
- use argv-based process spawning;
- capture and classify failures;
- restore prior state where feasible.

## 19. Time handling

Runtime correctness MUST NOT depend on changing the host system clock.

Network-time checking MAY report clock drift.

Changing host time or timezone is outside normal runtime behavior and, if retained at all, MUST require an explicit privileged administrative command.

## 20. State persistence

Persistent state MUST use versioned schemas.

Writes MUST be crash-safe:

1. write temporary file;
2. flush;
3. atomic rename where supported.

State corruption MUST not prevent startup; the application should quarantine invalid state and start with safe defaults when possible.

Migration from `dandelion_state.json` SHOULD be supported once, through an explicit importer.

## 21. Telemetry and logging

Use structured tracing.

Recommended stack:

- `tracing`;
- `tracing-subscriber`;
- optional metrics facade/exporter.

Every long-running component MUST expose health.

Useful metrics:

- scan duration;
- files discovered;
- chunks produced;
- queue depth;
- bytes sent;
- send errors;
- rate-limit delay;
- dropped/cancelled jobs;
- component restarts;
- source root availability;
- memory/RSS if available.

Logs MUST be rate-limited for hot errors.

## 22. Terminal UI

The TUI consumes snapshots; it does not own runtime state.

Recommended library: Ratatui.

Rendering MUST never block the transfer path.

Headless mode MUST be first-class and SHOULD be the service default.

## 23. Updater

Updater behavior MUST be separated from the transfer engine.

Preferred design:

- check signed/tagged GitHub releases;
- compare semantic versions;
- download to a temporary location;
- verify checksum/signature;
- stage replacement;
- use a controlled restart mechanism.

The running binary MUST NOT blindly overwrite itself.

Automatic update can be disabled by configuration.

## 24. Error model

Libraries SHOULD use typed errors with `thiserror`.

The application boundary MAY use `anyhow` for context aggregation.

Do not swallow operational errors with blanket `except/pass` equivalents.

Every ignored error class MUST be explicitly documented and metered/logged at an appropriate level.

## 25. Testing strategy

### 25.1 Unit tests

Required for:

- config parsing/validation;
- target policy;
- packet encode/decode;
- rate limiting;
- traffic budgets;
- state migration;
- path filtering;
- retry policies.

### 25.2 Property tests

Use property-based testing where useful for:

- packet round trips;
- malformed packet rejection;
- CIDR/target policy;
- chunk boundary conditions.

### 25.3 Integration tests

Network integration tests MUST use:

- loopback;
- local UDP receiver fixtures;
- Linux network namespaces where needed.

No CI test may require arbitrary public targets.

### 25.4 Parity tests

Create a Python-to-Rust parity ledger.

Each legacy capability receives one of:

- `mapped`;
- `redesigned`;
- `legacy-adapter`;
- `intentionally-removed`;
- `not-applicable`.

A feature is not considered migrated until its ledger item has evidence.

## 26. Performance methodology

Before claiming improvement, capture a Python baseline on the same hardware.

Benchmarks MUST include:

1. packet encode throughput;
2. sequential file chunk throughput;
3. scanner throughput on representative trees;
4. loopback UDP throughput;
5. CPU time per GiB processed;
6. peak RSS under backpressure;
7. queue latency;
8. graceful-shutdown latency.

Use Criterion for microbenchmarks and a dedicated end-to-end benchmark harness for transfer tests.

### 26.1 Initial acceptance gates

After functional parity for a benchmarked path:

- no throughput regression versus Python baseline;
- CPU per GiB MUST be materially lower on hot paths;
- memory MUST remain bounded under a stalled consumer;
- no unbounded worker growth;
- no unbounded queue growth.

Target engineering goal after optimization:

- at least 2x throughput on CPU-bound/dispatch-bound hot paths where network/device line rate is not already the bottleneck;
- at most 70% of Python CPU cost per GiB on those same paths;
- materially lower steady-state RSS.

These are engineering targets, not assumptions; benchmark evidence decides whether further optimization is justified.

## 27. Security and operational safety

Mandatory controls:

- WAN disabled by default;
- explicit target allowlist;
- bounded rate and traffic budgets;
- no arbitrary shell interpolation;
- no secrets in logs;
- no implicit root requirement;
- privileged actions isolated;
- config file permission checks where practical;
- dependencies audited;
- unsafe Rust prohibited unless an ADR documents the necessity, invariants, tests, and review.

Any `unsafe` block MUST include a `// SAFETY:` explanation.

## 28. Dependency policy

Prefer mature, maintained crates.

Candidate dependencies, subject to review:

- `tokio`
- `tokio-util`
- `serde`
- `toml`
- `clap`
- `tracing`
- `tracing-subscriber`
- `thiserror`
- `anyhow` at app boundary
- `bytes`
- `blake3`
- `rustix` / `nix`
- `rtnetlink`
- `ratatui`
- `crossterm`
- `reqwest` with rustls
- `semver`

Avoid dependency duplication and default features not in use.

## 29. CI and quality gates

GitHub Actions MUST run on every pull request.

Required gates:

- `cargo fmt --all -- --check`;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
- `cargo test --workspace --all-features`;
- architecture/dependency-direction check;
- Linux x86_64 build;
- Linux aarch64 build or cross-check;
- `cargo audit` or equivalent advisory check;
- `cargo deny` policy check;
- parity-ledger validation once the ledger exists.

Release builds MUST be reproducible enough to record:

- git SHA;
- Rust toolchain;
- target triple;
- dependency lockfile;
- checksums.

## 30. Code quality rules

- Rust 2024 edition unless a tooling constraint requires otherwise.
- Stable Rust by default.
- `rustfmt` canonical formatting.
- Clippy warnings are errors in CI.
- No giant all-purpose modules.
- No component may own unrelated platform, UI, networking, and persistence responsibilities.
- Public APIs require rustdoc when non-obvious.
- No magic sleeps in core algorithms.
- No detached unmanaged tasks.
- No silent infinite retry loops.
- No panics for expected runtime failures.
- Tests should use deterministic timing where possible.

## 31. Migration plan

### Phase 0 — Baseline and specification

Deliverables:

- this specification;
- source baseline pinned to commit SHA;
- Python capability inventory;
- parity ledger skeleton;
- benchmark plan.

Exit gate: architecture accepted and all existing feature domains classified.

### Phase 1 — Workspace/core/config/telemetry

Implement:

- Cargo workspace;
- core domain types;
- config;
- supervisor;
- cancellation;
- tracing;
- state skeleton.

Exit gate: headless process starts/stops cleanly with no transport implementation.

### Phase 2 — Scanner and packet pipeline

Implement:

- source roots;
- file discovery;
- chunk producer;
- packet v2;
- bounded channels;
- unit/property tests.

Exit gate: fixture files produce deterministic verified chunk streams.

### Phase 3 — Loopback transport

Implement:

- UDP loopback sender;
- local verification receiver fixture;
- pacing;
- progress snapshots;
- benchmark harness.

Exit gate: reliable local end-to-end transfer benchmark and graceful cancellation.

### Phase 4 — LAN and configured private/VPS transport

Implement explicit-target transports and private-network policy.

Exit gate: integration tests use only controlled local/private fixtures.

### Phase 5 — Authorized WAN transport

Implement:

- allowlist;
- traffic budget;
- retries;
- target health;
- WAN-disabled-by-default policy.

Exit gate: policy tests prove out-of-policy destinations cannot be sent.

### Phase 6 — Linux platform and drive lifecycle

Implement:

- mount discovery;
- removable-drive monitor;
- network interface state;
- connection summary;
- native Linux adapters.

### Phase 7 — Hotspot, TUI, updater, compatibility

Implement optional subsystems with strict boundaries.

Legacy packet support is added only if an actual migration need is demonstrated.

### Phase 8 — Cutover

Run Python and Rust baselines on identical workloads.

Complete parity ledger.

The Python implementation is not retired until required capabilities are verified or explicitly marked removed/redesigned.

## 32. Acceptance definition

The rewrite is complete only when all of the following are true:

1. every required Python capability is classified in the parity ledger;
2. required mapped/redesigned capabilities have test evidence;
3. all CI quality gates are green;
4. no production hot path uses an unbounded queue;
5. shutdown completes within the configured deadline under test;
6. memory remains bounded during a stalled transport test;
7. WAN is disabled by default and target policy is enforced in tests;
8. loopback and configured-target integration tests pass;
9. benchmark report compares Rust against the pinned Python baseline;
10. release artifacts include checksums and provenance metadata;
11. operational documentation covers config, service mode, troubleshooting, and upgrade/rollback.

## 33. Architecture decisions that require an ADR before changing

The following choices are considered architectural and MUST be changed through an Architecture Decision Record:

- Tokio vs a different runtime;
- packet v2 wire format;
- target authorization model;
- bounded-channel topology;
- persistence schema strategy;
- privileged-helper model;
- introduction of unsafe Rust;
- introduction of io_uring;
- updater trust/signature model.

## 34. First implementation task after this Spec

The first code PR SHOULD create the Cargo workspace and only the minimum vertical skeleton:

```text
config -> supervisor -> scanner fixture -> packet encoder -> loopback transport -> local verifier
```

It SHOULD NOT begin by porting all 3,180 Python lines.

That vertical slice must prove:

- configuration;
- component ownership;
- bounded backpressure;
- cancellation;
- packet encoding;
- local transport;
- telemetry;
- tests;
- benchmark wiring.

Only after that slice is green should additional feature domains be migrated.


## 35. Implementation record: 2026-09-28 first Rust CLI slice

The first native implementation uses eight concrete packages: app, core, config,
scanner, packet, pacing, transport and platform-linux. It implements the minimum
vertical slice specified in section 34. See ADR 0001 for the exact dependency
topology, packet format, Linux capability boundary and bounded runtime decisions.
There are no placeholder packages or fake success commands for later phases.

Implemented code: standard clap CLI; layered validated TOML configuration; finite
read-only scanner; bounded channel; BLAKE3-covered packet v2; monotonic integer
token bucket; positive per-run wire budget; owned real UDP loopback verification;
Supervisor cancellation/exception cleanup; tracing/JSON outputs and deterministic
exit codes. The Linux adapter supports secure beneath-root file opens and storage
reserve checking. It does not yet implement drive or network lifecycle discovery.

Quality infrastructure: x86_64/aarch64 native Actions jobs, formatting/Clippy/unit/
property/integration/CLI tests, stalled-consumer cancellation and RSS sampling,
source immutability, dependency auditing/licensing policy, architecture and parity
ledger checks, and checksummed build artifacts with toolchain/commit metadata.
The existence of these jobs is not evidence of success: inspect the exact run SHA.

The canonical ledger now separates classification from implementation state and
links evidence files. Full baseline/cutover performance gates remain open. LAN,
VPS/WAN authorization, persistent state/resume, drive monitoring, hotspot, TUI,
automatic updating and legacy compatibility are pending. No claim of complete
Python migration or numerical performance improvement is made by this slice.

Storage is a non-negotiable operational constraint: estimate peak writes on each
filesystem, preserve at least max(1 GiB, 5% capacity), and stop when information is
missing or reserve cannot be maintained. Heavy builds/testing belong in CI.

First-slice hardening: stdout broken pipes are handled only at the output boundary,
not used to hide transport I/O errors. Shell completions are generated into a
bounded-size command-derived buffer before stdout writing. Added closed-pipe and
source-shrink regression tests; an interrupted self-test preserves cancellation
status rather than reclassifying cancellation as data corruption.

First-slice dependency refinement (ADR 0002): SourceAccess/SourceRoot/SourceFile
ports and opaque FileStamp live in core; the app injects LinuxSourceAccess. The
scanner has no production dependency on platform-linux. Linux identity details
and safe-open primitives remain entirely inside the adapter. Tests exercise the
real adapter via dev dependencies. This supersedes ADR 0001's initial direct edge.

Packaging refinement: pin maintained Node 24 checkout/upload Actions to verified
commit SHAs. Package binaries inside tar.gz to preserve executable permissions
through artifact ZIP transport; checksum both the archive and all inner evidence
files. Build metadata records the actual Actions checkout revision.

CI repair after run 36419586202: both native quality jobs passed, but the pinned
cargo-audit 0.21.2 could not parse CVSS 4.0 entries in the current RustSec database.
Upgrade to published cargo-audit 0.22.2 (lockfile auditing only, no binary-scanning
feature needed), retaining audit as a mandatory gate. Separate advisory and
license/source checks so both failures are visible. Add extraction, executable
mode, inner checksum and packaged self-test checks before artifact upload.
