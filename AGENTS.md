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
