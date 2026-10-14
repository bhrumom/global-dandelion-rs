# ADR 0003: durable traffic leases and supervised service

Status: accepted. Spec v2 C01/C13/C15/C19/C22.

A private directory holds a stable exclusive writer lock and a versioned checksummed
state envelope. Commit uses same-directory temp + sync_all + atomic persist +
directory sync. The OS releases the writer lock on crash. Corruption/future schema
never silently resets history. A conservative whole-run lease is durably charged
before network startup; only a successfully settled runtime may refund provably
unused bytes. Errors/crashes retain the entire lease. This trades exact lost-ACK
accounting for a strict no-budget-reset invariant without per-packet fsync.

Legacy traffic_state.json (current_month, total_sent_bytes) is imported explicitly
and idempotently as a lifetime floor. No automatic monthly reset. File/byte-level
resume is separate work and is not claimed by this traffic ledger.

Repeat/watch share one session budget and a single token bucket across passes;
restarting a scan does not mint another burst. An explicit UTC offset selects
day/night rates while monotonic clocks schedule work. The TUI is a latest-value
observer on an owned blocking worker and never owns transfer state.
