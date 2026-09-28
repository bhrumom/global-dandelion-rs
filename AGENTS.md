# AGENTS.md

## Source of truth

Before implementing or reviewing this repository, read:

`docs/specs/global-dandelion-rust-architecture.md`

The Spec is authoritative for architecture, migration policy, concurrency, target authorization, testing, and acceptance.

## Non-negotiable rules

- Do not mechanically translate the Python file line by line.
- Do not recreate `CPU_COUNT * 50` worker scaling.
- Do not introduce unbounded production queues.
- Do not introduce mutable global application state.
- WAN remains disabled by default and requires explicit authorized targets.
- CI tests must not transmit to arbitrary public hosts.
- Use bounded channels/backpressure and explicit cancellation.
- Keep Linux privileged operations isolated from the core.
- Any `unsafe` Rust requires an ADR, tests, and a `// SAFETY:` invariant comment.
- Every migrated legacy behavior must be recorded in the parity ledger.
- Performance claims require measurements against the pinned Python baseline.

## Development sequence

Follow the migration phases in the Spec. The first implementation slice is:

`config -> supervisor -> scanner fixture -> packet encoder -> loopback transport -> local verifier`

Do not expand into later feature domains until that slice is green in CI.

## Disk and execution safety (mandatory)

Before every write, dependency resolution, build, download, extraction, cache or
artifact operation, check all affected filesystems, estimate peak additional
storage including temporary output, and preserve max(1 GiB, 5% capacity).
Fail closed on unknown capacity, unknown estimate or insufficient reserve.
Re-check between stages. Never exhaust a user's disk, and never delete unrelated
user data/caches/processes to make space. `scripts/disk_guard.py` is the shared
preflight helper; runtime temporary writes have the same policy.

Heavy compilation, tests, dependency audit tool installation and packaging must
run in GitHub Actions or an explicitly provisioned disposable runner, not on
persistent user computers/VPS. Formatting, source edits and lockfile resolution
may be done locally only after their bounded-write preflight.

## Implementation and evidence

- Keep the executable a native Rust binary (`dandelion`), not a Python wrapper.
- Preserve CLI stdout/stderr separation, stable exit codes and shell completion.
- Use `docs/migration/parity.json` as the machine-readable migration inventory.
- `implemented` is not `full parity verified`; benchmark gates remain separate.
- Never mark absent domains complete or add no-op compatibility commands.
- Update Spec development notes, ADR and ledger with every architecture change.
- Do not claim a CI check passed unless the exact committed revision is green.
