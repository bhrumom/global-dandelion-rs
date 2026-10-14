# ADR 0010: bounded authorized public target pools

Status: accepted and implemented. Date: 2026-09-29. Spec revision 2.1 target-policy refinement.

## Context

The pinned Python baseline can generate public destinations. Reproducing an
unrestricted Internet-wide generator would conflict with the replacement safety
contract. Spec 2.1 instead requires selection only inside explicitly configured
address/CIDR pools for which the operator is authorized to transmit workload data.
Generation must remain separate from authorization.

## Decision

`dandelion-core::targets::TargetPool` parses at most 64 canonical public CIDR
entries. Every entry binds a destination port and ASCII TLS server name. Invalid,
private, loopback, link-local, multicast and overlapping same-port ranges fail
closed. Pool cardinality is represented as ranges plus a bounded integer count; the
implementation never expands a CIDR into an in-memory address vector.

`TargetSelector` is a separate state machine. It supports:

- sequential selection;
- randomized selection using process-local standard-library random hash seeding;
- deterministic-seeded selection for reproducible tests and controlled operations.

A selector emits at most eight unique targets per session and refuses to emit more
unique addresses than the configured pool contains. The randomized permutation is
implemented as an offset plus a coprime stride over pool cardinality, so memory use
is independent of CIDR size.

The configuration layer requires WAN enablement, bounded target_count, compatible
seed/mode combinations and no ambiguous overlap between explicit peers and pool
addresses. Explicit exact WAN peers continue to use the existing exact allowlist.

The application composition root selects concrete peers immediately before a TLS
session and then calls `authorize_with_pool`. Selection therefore does not mint an
authorization token. Every generated SocketAddr is validated again against unicast
rules and the effective configured pool before `dandelion-transport` is allowed to
connect. `platform time --peer` uses the same composition helper instead of a
parallel authorization path.

Selected concrete addresses become ordinary authenticated target-health entries in
the runtime snapshot, and the selection mode/pool cardinality/concrete choices are
also emitted through structured tracing. Resume scope already hashes serialized
transport policy, so pool, mode, count or seed changes invalidate an old checkpoint
scope.

## CI and safety

Core tests prove bounds, deterministic replay, scope rejection and authorization
revalidation. Configuration tests prove fail-closed mode/seed/WAN validation. An
application test invokes the same shipping composition helper used before real TLS
connection and verifies selected addresses remain inside the pool.

CI does not connect to or send workload data to public addresses. Real external
transfer still requires operator-provisioned mutual TLS credentials, explicit WAN
consent, an authorized pool or exact allowlist, and the existing traffic/time
budgets.

## Consequences

This settles the legacy `generate_ip_from_cidr` / `prepare_target_ip_pool`
responsibility without restoring unsolicited Internet spray. Weighted selection is
not implemented because Spec 2.1 marks it optional and no production requirement
currently justifies extra policy complexity.
