# Global Dandelion: Complete Native Rust CLI Architecture and Acceptance Spec

Revision: **2.1**. Updated: **2026-09-29**. Status: **authoritative implementation contract**.
This document REPLACES revision 1; it does not append another first-slice plan.
Revision 1 is preserved only in `docs/history/architecture-v1.md` for provenance.

Repository: `bhrumom/global-dandelion-rs`.
Python baseline: `bhrum/global@aa7f3720c42856f0e1d18742f78fba156227ee79`.
Source: `global_dandelion.py`, blob `54bf6226eb80d1ef6c4f6087fb02579141ab134f`, 3,179 lines.
Rust starting baseline: `7a0f78d9415509fbe0a49b183ee12b59640beef4`.
Verified starting run: 36421256894, both native architectures and mandatory gates green.

## 1. Product goal and non-negotiable completion rule

Deliver a production-quality native Rust CLI that replaces every required,
legitimate capability of the Python program. A working loopback example is NOT
completion. Documentation, empty modules, mocked acceptance, successful builds,
or checklists without production wiring are NOT feature implementation.

No architecture is universally best. For this workload, choose a modular monolith
with inward dependency direction, explicit capability ports, a bounded streaming
data plane and a supervised control plane. Complexity must buy a tested behavior.
The design optimizes correctness, predictable memory/storage, testability,
observability, safe operations and maintainability before peak throughput.

No guarantee of defect-free software is inferred from the word complete. Every
completion statement must identify the exact tested commit and remaining risks.

## 2. Source-of-truth order and change control

1. This Spec defines the required product and acceptance contract.
2. `docs/migration/parity.json` defines each legacy responsibility and evidence.
3. ADRs define reviewed architecture/protocol decisions.
4. Production code and behavioral tests prove implementation.
5. CI and real-device evidence prove the exact tested revision.

Do not weaken a required gate, remove a requirement, substitute a fake adapter,
add `continue-on-error`, or relabel pending as not-applicable to make CI green.
`implemented` and `verified` are different. A planned feature is never advertised
as usable. Preserve predecessor evidence, but invalidate verification after changes.

## 3. Capability coverage (all domains must be settled)

| ID | Legacy responsibility | Required Rust outcome | Acceptance |
|---|---|---|---|
| C01 | main / worker management | one Supervisor, owned tasks, bounded concurrency | lifecycle and injected failure tests |
| C02 | global constants / switches | validated TOML/env/CLI config, explicit modes | precedence, malformed/contradictory input tests |
| C03 | file scanner | selected roots, filtering, bounded traversal/read | fixture, mutation, empty/non-UTF8 files |
| C04 | metadata scanner | explicit read-only metadata scan command | no payload reads; budgets and cancellation |
| C05 | surface scanner | explicit paced read-only verification pass | finite budgets, source immutability |
| C06 | source auto discovery | observable mount inventory plus opted-in roots | fake mount table, startup/removal/addition |
| C07 | external drive monitor | bounded mount-diff observer, rescan lifecycle | plug/unplug and inaccessible root behavior |
| C08 | loopback sender/receiver | real packets, receive verification | exact content, boundaries, cancellation |
| C09 | LAN | configured authorized unicast peers | no discovery-to-send escalation; local fixture |
| C10 | private/VPS | explicit authenticated peer sessions | target rejection, receipts, failures |
| C11 | WAN | off by default, explicit allowlist and consent | cannot send outside effective policy |
| C12 | packet framing | versioned, bounded, validated protocol | roundtrip, truncation, fuzz/property, corruption |
| C13 | constant/day/night pacing | monotonic limiter + explicit schedule policy | deterministic time and transition tests |
| C14 | traffic accounting | attempts/reservations/kernel/verified distinct | fanout/retry/overflow/budget tests |
| C15 | persistent state | versioned, locked, atomic durable store | corruption, concurrent writer, migration, crash tests |
| C16 | resume | bounded idempotent checkpoint, source identity | interrupted/restarted versus clean result |
| C17 | network interfaces/connections/neighbours | observational platform inventory | parser fixtures; no active arbitrary probes |
| C18 | time metadata | read-only clock metadata/drift observation | no date/hwclock/timezone mutation |
| C19 | terminal display/headless logging | immutable snapshots, TUI optional | no data-path stalls, terminal restore, JSON separation |
| C20 | hotspot | optional explicit privileged capability | plan/apply/status/stop, rollback and hardware gate |
| C21 | update/version | semver, verified release staging/rollback | signature/hash/size/version rejection; no self-overwrite |
| C22 | legacy state/database | bounded explicit import and metadata lookup | pinned fixtures, no implicit target generation |
| C23 | rename effect/recovery | opt-in journaled compatibility, never default | interruption recovery, no overwrite/data loss |
| C24 | benchmark/cutover | equivalent pinned Python and Rust workloads | same hardware, raw evidence, no unsupported speed claims |
| C25 | release/service operations | reproducible inputs, checksums, provenance, docs | extracted binary tests, install/upgrade/rollback |

The existing 37-row legacy ledger MUST retain every symbol/domain; this table
adds acceptance detail, not a reduction of scope. Optional compatibility/hardware
features still need explicit disposition, rationale and evidence.

## 4. Deliberately excluded unsafe defaults

Never restore broadcast-to-discovered strangers, implicit root elevation, hidden
clock/timezone mutation, unlimited queues or CPU_COUNT * 50 worker explosions.
Randomized public-target selection MAY be supported, but only inside explicitly
configured address/CIDR pools for which the operator has authorization to transmit
workload traffic. The unrestricted public Internet is never an implicit authorized
pool. WAN, hotspot, drive enrollment, source rename and update installation are
opt-in. CI must never send workload data to arbitrary public hosts or mutate host
networking.

## 5. Repository and dependency architecture

```text
crates/
  dandelion-app/             CLI and composition root; Supervisor
  dandelion-core/            values, policies, capability contracts, pure telemetry/rate models; no Tokio/Linux
  dandelion-config/          deserialization, merge, validation
  dandelion-scanner/         traversal, bounded readers; source capability only
  dandelion-packet/          deterministic wire codec and compatibility codecs
  dandelion-pacing/          token bucket and schedule policy
  dandelion-transport/       owned sockets, authorization, verified delivery
  dandelion-state/           versioned durable state; no UI/transport ownership
  dandelion-platform-linux/  filesystem/mount/interface/process capabilities
  dandelion-tui/             optional observer over core snapshots; never owns transfer state
  dandelion-hotspot/         isolated lifecycle capability
  dandelion-updater/         trust verification and staging only
  dandelion-compat/          explicit bounded legacy adapters; never authorization policy
```

Create a package only with an implemented responsibility. CLI entrypoints must
not contain orchestration or platform business logic. No cyclic dependencies.
Core defines SourceAccess/SourceRoot/SourceFile and immutable FileStamp contracts;
app injects adapters. New process, state, update and time adapters follow the same
inversion rule. One module must not own unrelated network/UI/state/privilege work.
Static architecture checks include production and feature-specific dependencies.
Unsafe application Rust is forbidden; any exception needs an ADR, invariants,
tests and explicit review rather than a lint suppression.

## 6. Data plane, ownership and resource bounds

```text
source capability -> scanner/read producer -> bounded chunk queue
 -> codec -> budget reservation -> monotonic pacer -> authorized transport
 -> verified receipt -> immutable progress snapshot / checkpoint
```

A single logical scan feeds all selected destinations. Never read the whole tree
per network worker. Fanout is bounded and counted per destination. Reuse encoded
buffers; borrowed decode payloads must not outlive their datagram. Regular-file
I/O and mount walks run on a bounded blocking executor, not Tokio reactor threads.
Limit roots, depth, open directories/files, queue slots, payload bytes, metadata
rows, receiver sessions, cache entries, retries, logs, and temporary disk writes.
A byte-memory budget is not a claim about total RSS; measure RSS separately.

All payload producers receive backpressure. Telemetry uses latest-value snapshots
with explicit overwrite semantics, never an unbounded history. Exhausting a budget
is a typed non-success outcome, not silently dropping work or creating more workers.

## 7. Supervisor and shutdown contract

States: Created -> Validating -> Starting -> Running -> Draining/Cancelling
-> Persisting -> Stopped, with terminal Failed/Cancelled/BudgetExhausted outcomes.
One owner decides lifecycle; child tasks do not call process::exit. SIGINT/SIGTERM,
receiver failure, source failure and budgets reach the same shutdown path.

Stop new scan work, close consumer endpoints to release blocking_send, cancel
children, settle joins by one absolute monotonic deadline, flush bounded durable
state, restore UI, then return a stable exit code. No detached unmanaged task.
Kernel-blocked filesystem calls cannot be force-cancelled by aborting spawn_blocking;
report this limitation and timeout, do not wait indefinitely on runtime drop.
The library's caller-drop behavior must also close queues and signal cancellation.

## 8. Configuration and CLI contract

Defaults < explicit TOML < DANDELION__SECTION__KEY < provided CLI overrides.
`DANDELION_CONFIG` selects config. Unknown fields or unsupported combinations fail
before sockets/privileged changes. Secrets use permission-checked files or a secret
provider, not argv, logs, config dumps or durable snapshots. Limit config size.
Internal paths use OS strings; never silently corrupt non-UTF8 paths.

Required command families: run, config init/check, self-test, doctor, status,
scan (metadata/read), platform (mounts/interfaces/connections/neighbours/watch),
receive, state inspect/import, hotspot plan/apply/status/stop, update check/stage,
compat recovery, benchmark, completions. A family appears in --help only when it
has real behavior; do not invent commands returning success for pending features.

Linux deployment MUST support unattended headless operation through a hardened
systemd service. The service MUST be enable-able at boot, remain supervised by
systemd rather than self-daemonizing, restart only under a bounded documented
policy after unexpected failure, write structured logs suitable for journald,
recover durable state/checkpoints, reject duplicate ownership, and honor the same
graceful SIGTERM shutdown deadline as foreground execution. Enable/disable/start/
stop/restart remain explicit operator actions; installation MUST NOT silently enable
network transmission.

Machine results -> stdout; human diagnostics/structured tracing -> stderr.
Noninteractive/headless usage is first class. Help/version/completion never need
network/root. Handle broken stdout only at output boundaries, not as transport
success. Document all exit codes and behavior under scheduler/service execution.

## 9. Source safety and drive lifecycle

Explicit trusted source roots. Rooted handle-based opens with no symlink/magic-link
escape, mount crossing or special-device/FIFO reads. Use Linux openat2 fail-closed;
never weaken into plain path opens on an unsupported kernel. Before/after stamps
come from the same file handle. Stable snapshots/checkpoint identity must not rely
only on mutable path names. Ignore atime when detecting read-induced metadata changes.

Canonicalize/deduplicate selected roots without retaining the entire tree. Directory
walking must be streaming; sorting all entries or collect-to-close is disallowed.
Hidden/generated/config/state/update directories are excluded by documented policy.
Never scan secrets merely because a drive appeared. Discovery reports candidates;
enrollment requires configured allowed mount roots/devices. Mount removal must not
redirect a resumed job into a replacement disk at the same path.

Metadata scan reads metadata only; surface scan reads selected regular files at
an explicit finite rate/read budget. Repeated service scans require a configured
interval and cancellation; never a busy-spin loop or unlimited implicit daemon.

## 10. Transport, authorization and protocol

Keep the proven local loopback adapter. External peers are validated numeric
SocketAddr values from configured policy; reject unspecified/multicast/broadcast,
zero ports, out-of-policy targets and implicit DNS expansion. Private, VPS and WAN
classes are explicit; a public target requires WAN enabled and authorization by the
effective target policy.

For explicitly authorized public address/CIDR pools, the target-policy layer MAY
expose a bounded TargetPool and a separate TargetSelector supporting sequential,
randomized, deterministic-seeded and weighted selection. Generation does not grant
authorization: every selected address MUST be revalidated against the effective
policy before session establishment. Randomized selection MUST be reproducible in
tests, observable in status/telemetry, bounded in memory, and unable to widen its
configured authorization scope.

Sending file payload to external peers requires authenticated session establishment,
matching receipts and bounded timeouts. A checksum alone is not authentication.
Prefer standard TLS/QUIC or a reviewed authenticated envelope over existing UDP;
any custom envelope needs an ADR, freshness/replay rules, MAC checks, bounded
session tables and an explicit no-encryption caveat. Never claim durability when
a peer only verified bytes in memory. Receiver never creates a reflection amplifier.

Version 2 framing remains documented in docs/protocol-v2.md. Any envelope overhead
is included in the UDP size limit, pacing and attempted traffic budget. Control
traffic, fanout and retries also count. Retries cannot reset the lifetime budget;
ambiguous sends are conservatively charged, not refunded into overshoot.
All retries have cancellation, finite count, backoff and typed error classification.
A transport error cannot be swallowed or relabeled completed.

## 11. Pacing and clock policy

Token bucket uses integer accounting and monotonic time. Constant rate and explicit
day/night schedule are distinct configuration choices. Civil-time selection uses
an explicit timezone/offset; all delays/deadlines continue to use monotonic time.
Schedule boundary/overnight range/clock-adjustment tests are required. Global budget
is conserved across targets/retries and, when configured, across restart.
Network time is optional metadata only; never needed for correct runtime scheduling.

## 12. Persistence, import and recovery

Versioned schema; single writer lock; conservative traffic reservations; no secrets.
Write same-directory temporary file, sync file, atomically rename and sync directory.
Account for peak temp plus prior file plus journal storage before writing. Detect
truncation/corruption/version mismatch. Unknown future schemas fail closed. Corrupt
history must not silently reset a traffic limit; quarantine only through explicit
recovery preserving original evidence. A new safe-state mode must not erase debt.

Legacy traffic_state.json import is explicit, bounded, schema checked, idempotent
and no-overwrite by default. Imports never authorize network peers. Checkpoints
bind source identity, config/target policy and acknowledged position; conservative
replay cannot become duplicate durable effects. Crash tests distinguish reserved,
sent, acknowledged and committed states. Whole-file resume may be used initially
only when honestly documented; do not call byte-level restart complete without tests.

C16 uses `GDAN/file-resume/v2`. Before a resume pass, any allowed-mount discovery is
frozen to the exact currently mounted roots. The scope binds every canonical source
root to its Linux mount ID, parent ID, device, mount root, mountpoint, filesystem,
source and mount options, in addition to target policy and trust material. Unmount,
remount or device replacement therefore invalidates old checkpoints conservatively
instead of applying old file completion to a new object mounted at the same path.
Per-file keys still bind opened-file size/device/inode/mtime/ctime. Hardware hotplug
proof remains a separate real-device gate.

## 13. Telemetry and terminal lifecycle

Expose phase, component health, source availability, queue/backlog, completed files,
attempted/reserved/kernel-accepted/verified bytes, rate, target health, retries and
errors. Snapshot rendering cannot block sending. TUI reads snapshots; terminal
restoration must run on normal return, cancellation and error. Non-TTY output stays
headless; no escape sequences on redirected machine output. No terminal-owned state.

## 14. Linux inventory and privileged hotspot boundary

Mounts, disk usage, interfaces, TCP states and neighbours are read-only, parser-
fixture tested and bounded. Do not confuse an incomplete proc/sys view with zero.
Privileged hotspot work is a separate optional adapter, never a startup side effect.

Hotspot plan must show interface, profile and intended changes. Apply requires
explicit CLI/config consent and an existing capability (no hidden sudo). Prefer
NetworkManager D-Bus; a narrow timeout-bounded argv adapter is acceptable. No shell
interpolation, secrets in argv, arbitrary radio-country/DFS forcing or interference
behavior. Persist rollback intent, scope cleanup to owned resources, and restore
previous interface/profile state when feasible. Real wireless hardware acceptance
is mandatory before claiming hardware parity; a fake-process test is not that gate.

## 15. Update and release trust boundary

Separate discovery, verification, staging and operator-controlled activation.
Use semantic versioning; stable channel must not silently install prereleases.
Validate trusted origin, bounded manifest/artifact sizes, architecture/toolchain
requirements, trusted signing key, digest and downgrade/rollback policy. A checksum
from the same untrusted source is not a signature trust chain. Never blindly execute
a downloaded script, git-checkout running code or overwrite the active executable.
Staging is atomic and disk-preflighted; interrupted staging is safely removable.
Activation/rollback is a separate explicit operator/service-manager operation.

## 16. Compatibility boundaries

Legacy MD5 packet layout, geographic/ASN metadata, state import and rename recovery
live in named adapters only when required. MD5 is legacy framing, not cryptographic
security. CSV databases may label an already authorized target; must not generate
or implicitly authorize arbitrary addresses. Bound input size/rows and malformed
CIDR handling. Preserve licensing/source provenance of imported datasets.

The pinned RenamerThread exercises hidden hard/symbolic links, not original filenames.
This optional link effect remains disabled by default. Any retained implementation needs a
durable intent journal, no-overwrite rename, exclusion of repo/config/state, safe
open-handle semantics and crash recovery. Unknown renamed files must not be guessed
or bulk-renamed. Never alter user data to simulate program progress.

## 17. Disk, privilege and execution safety

Before writes/downloads/builds/extraction/state/updates, check all affected mounts,
estimate additional peak data including caches and temp files, and preserve
max(1 GiB, 5% filesystem capacity). Unknown capacity/estimate or insufficient reserve
fails closed. Recheck between stages. Never delete unrelated user data/caches or
kill processes to create room. Heavy builds/tests/packaging run on disposable GitHub
Actions runners, not persistent user workstations/VPS. No real payments/OTP needed.

## 18. Test pyramid and mandatory CI gates

Unit and deterministic clock tests; codec property/malformed-input tests; target
policy negative tests; real local socket integration; filesystem race/mutation;
state crash/corruption/concurrent writer; process signal/shutdown under backpressure;
terminal restoration; injected subprocess failure/timeout; secure updater tamper;
packaged artifact extraction/permissions/checksums/execution; controlled benchmarks.
All adapters must be wired through the real CLI in at least one integration test.

Every PR and main push: fmt, Clippy -D warnings, all workspace/all-feature tests,
architecture check, machine parity validation, RustSec audit, cargo-deny, native
Linux x86_64/aarch64 build and tests, systemd unit/install lifecycle validation, and
final required acceptance gate. Service tests MUST cover non-TTY startup, duplicate
instance prevention, bounded restart behavior, SIGTERM graceful shutdown, journald-
suitable output separation and state recovery after a supervised restart. Pin
toolchain, lockfile and Actions SHAs. Never report green from a previous HEAD.
Preserve raw results and failure evidence. Optional hardware gates must be visibly
outstanding.

## 19. Benchmark equivalence and acceptance

Pin Python source and workload. Never execute Python's side-effectful main in CI.
Extract reviewed pure scan/chunk/encode helpers into an isolated benchmark harness.
Use same host, file fixture, payload sizes, integrity work and delivery verification.
Distinguish legacy MD5 and new BLAKE3 protocol costs. Record repetitions, distributions,
wall/CPU time per GiB, actual peak RSS, backlog, shutdown, small/large files and
hardware/kernel/toolchain/commit metadata. Shared-runner microbenchmarks are not a
universal speed guarantee. Targets (2x dispatch-bound throughput and <=70% CPU/GiB)
remain engineering goals, never assumed results or reasons to falsify reports.

## 20. Final acceptance and migration order

First-slice gate is ALREADY satisfied by run 36421256894; proceed beyond it.
Next increments: durable state and platform inventory; schedule/telemetry/service;
authorized external transport and receiver; optional UI/hotspot/updater/compat;
equivalent benchmarks and full cutover. Each increment must be executable/tested,
not an excuse to declare the remaining product out of scope.

Full replacement requires every required capability settled, production wiring,
exact-HEAD green CI, real authorization rejection, bounded memory/storage,
restart/cancellation/corruption tests, verified boot-enabled/background service
lifecycle, appropriate real-device hardware proof, controlled baseline report,
versioned releasable artifacts and operational docs.
A complete gate must fail while required pending rows/hardware proofs remain.
A foundation gate may pass independently but must not be named full completion.

External acceptance evidence is fail-closed. The evidence manifest must bind the
exact tested commit and contain exactly the five required gate IDs. Each gate entry
must name the same gate ID, point to a bounded regular JSON report, provide that
report's SHA-256, record `result = pass`, and name an independent reviewer. The
report itself must bind the same gate ID and exact commit, record `result = pass`,
and identify the performer. `reviewed_by` must differ from `performed_by`.
`scripts/check_parity.py --require-complete --evidence FILE` re-reads every report
without following symlinks and verifies the digest and bindings. A handwritten
manifest that merely says `pass`, a stale commit, a missing report, digest mismatch,
gate-ID mismatch or self-review MUST fail the complete gate.

Gate reports are not generic pass envelopes. The verifier enforces gate-specific
semantics before full replacement can become true: current-ci must bind both native
artifacts, their BUILD-INFO and packaged python-rust-comparison commit, GitHub ZIP,
outer archive and binary digests, checksums and self-tests to the exact commit; hotspot
must identify real non-virtual Wi-Fi hardware and prove apply/interruption/rollback;
hotplug must identify original and replacement physical devices and prove old
checkpoint rejection at the same mount path; controlled-cutover must contain raw
same-fixture Python/Rust samples with complete transfer integrity and MUST cross-bind
its architecture, artifact ID/name, GitHub ZIP digest, outer archive digest, binary
digest and workflow/BUILD-INFO/comparison commit to the exact corresponding current-ci
artifact; release trust
must identify a non-synthetic production signer/origin and prove staging, activation,
health, rollback and post-rollback health. A minimal JSON object that only says pass
is invalid even when its digest is correct.

Reviewer metadata is also bound to the exact report SHA-256 and tested commit, with
an approve decision and immutable external review reference. The local verifier
re-fetches the canonical scoped GitHub review and checks actor, approval state,
exact commit, attestation body digest and performer/reviewer separation. The external
acceptance process remains responsible for confirming that claimed physical
observations actually occurred and that off-GitHub performer identity is truthful.

## 21. Implementation records

Historical first-slice implementation notes are in docs/history/architecture-v1.md.
New development/validation notes belong in docs/development/, referencing Spec IDs,
commit/run IDs, implemented behaviors, tests and unresolved acceptance requirements.
The migration ledger remains the authoritative current implementation/evidence index.

## Primary engineering references

- Tokio graceful shutdown: https://tokio.rs/tokio/topics/shutdown
- Rust File durability and locks: https://doc.rust-lang.org/std/fs/struct.File.html
- rustix rooted openat2: https://docs.rs/rustix/latest/rustix/fs/fn.openat2.html
- NetworkManager nmcli: https://networkmanager.pages.freedesktop.org/NetworkManager/NetworkManager/nmcli.html

These references inform implementation mechanics; the contracts above are project
requirements and are not claims that any external source certifies this product.


### Reviewer attestation hardening (evidence contract v2)

Full-replacement acceptance MUST NOT trust a manifest-provided reviewer name or arbitrary reference string. Each gate report now carries both `performed_by` (execution context) and `performed_actor` (the accountable operator/automation principal). The independent reviewer must differ from both.

For a real `--require-complete` verification, every gate entry must reference a live GitHub pull-request review API URL of the form `https://api.github.com/repos/{owner}/{repo}/pulls/{pr}/reviews/{id}`. The verifier re-fetches that review, requires state `APPROVED`, requires its `commit_id` to equal the exact tested commit, and requires the returned GitHub login to equal `reviewed_by`.

The GitHub review body itself is a JSON attestation containing `schema_version=1`, the exact `gate_id`, exact `tested_commit`, exact gate-report SHA-256 and `decision=approve`. The manifest pins the exact UTF-8 review body with `review_body_sha256`; an edited, dismissed, stale, nonexistent, non-GitHub or mismatched review therefore fails closed. CI self-tests inject a synthetic review resolver only for contract tests; real full-replacement verification uses the live GitHub API.


### Reviewer scope pinning (evidence contract v3)

A live APPROVED review is insufficient if it can be replayed from an unrelated
repository or pull request. The evidence contract therefore fixes a canonical
`review_scope` for this migration: repository `bhrumom/global-dandelion-rs`,
pull request `#1`. Every `review_reference` MUST target that exact scope.

The verifier rejects a foreign repository or different pull request before resolving
the review, then requires the fetched GitHub review's `pull_request_url` to equal
the canonical PR API URL. The synthetic contract test includes both replay negatives.
This closes cross-repository/cross-PR reviewer-attestation substitution without
claiming that GitHub identity alone proves the truth of physical observations.


### Live current-CI resolution (evidence contract v4)

A report-level `workflow.run_id`, job result, or artifact digest is not sufficient proof by
itself because those fields can be copied into a handwritten JSON report. For the
`current-ci` gate, real `--require-complete` verification MUST resolve the configured
canonical GitHub repository again and verify the reported Actions run from GitHub.

The live run MUST be a completed successful `pull_request` run for canonical PR #1,
have `head_sha` equal to the exact tested commit, and execute
`.github/workflows/ci.yml`. The verifier MUST re-fetch the run's jobs and require
exactly one successful x86_64 quality job, aarch64 quality job, dependency-policy job,
and required-acceptance-gate job.

For each required native artifact, the report's artifact ID and name MUST resolve in
that same run. GitHub's live artifact metadata MUST report a non-expired, non-empty
artifact whose `digest` equals the report's GitHub ZIP SHA-256 and whose workflow
run ID/head SHA bind the same exact run and tested commit. Inner archive, packaged
binary, BUILD-INFO and comparison digests remain independently inspected report
material and are still bound by the reviewer attestation.

Synthetic contract tests inject a fake Actions API resolver only to exercise positive
and negative verifier behavior. Real full-replacement verification uses the live
GitHub API. Tests MUST reject a stale live head, a non-PR run, and a mismatched live
artifact ZIP digest.

This is evidence contract v4. It closes the remaining path where `current-ci` could
look structurally valid while its Actions facts existed only in a handwritten report.
Physical hotspot, hotplug, cutover observations and production trust still require
real execution plus independent review; GitHub metadata is not treated as proof of
physical events.
