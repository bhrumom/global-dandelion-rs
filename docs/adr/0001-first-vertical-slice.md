# ADR 0001: bounded Linux-first correctness slice

Status: accepted for first implementation; performance/cutover gates remain open.
Date: 2026-09-28.

## Decision

Implement the existing Spec's first vertical slice before expanding domains.
Use a Rust 2024 workspace, pinned Rust 1.98.1 and a committed lockfile. Eight
packages correspond to current production responsibilities; no placeholder
hotspot/updater/TUI packages exist. Public binary name: `dandelion`.

One blocking scanner owns traversal/file reads, one bounded Tokio MPSC owns
queued chunks, one Supervisor owns dispatch/rate/budget and final statistics.
The transport trait is the explicit injectable boundary for receiver failure and
stalls. Production has two async runtime threads and at most two blocking workers;
there is no CPU-count-derived thread explosion, mutable global application state,
unsafe application Rust or detached background receive task.

Dependency direction is checked by `scripts/check_architecture.py`. Core uses only
standard-library domain values, serde and typed errors. Scanner -> Linux adapter
is a deliberate first-slice dependency: safe source opening is mandatory, not a
portable `File::open` fallback. It is a small capability boundary, not a global
platform service. The scanner is Linux-specific until a source-access port is
introduced in a separate reviewed change.

Packet v2 uses an 80-byte header and at most 1,152 payload bytes, for a maximum
UDP payload of 1,232 bytes. This is a conservative IPv6-minimum-MTU-oriented
budget, not a guarantee for every tunnel or path. Byte order is big endian.
Checksum covers header metadata and payload. An empty file has a terminal empty
frame. Legacy MD5 framing is not silently accepted; no authentication is claimed.

The initial transport is a **stop-and-verify** connected loopback socket pair.
No infinite receive loop, public targets, random destination selection, or
ambiguous send-only success. This is a correctness reference; it is not a claim
of peak UDP throughput. Optimized batching and authenticated external transfers
require their own tests and ADR before being enabled.

## Bounds and cancellation

A queue capacity and payload size are validated together. A scanner can hold a
producer chunk while waiting for space. The consumer has one chunk/encode buffer;
the receiver has one bounded datagram buffer. WalkDir must not sort all entries
or collect siblings to close handles: max_open is max_depth + 1 (at most 65).
Roots are limited to 64 and de-duplicated, including nested roots.

Shutdown drops the queue receiver first, cancels the worker, and awaits its join
with a configured timeout. The transport is also wrapped by a Supervisor select
so a faulty adapter cannot suppress cancellation. No automatic retries or packet
replays obscure budget accounting. The runtime is dropped with zero additional
shutdown wait after the Supervisor has joined/settled its worker. Kernel-level
blocked I/O remains an OS limitation and is reported, not mislabeled success.

## Source and disk safety

File opens use Linux `openat2` beneath an already open root, with no symbolic
links, magic links or mount crossings. Final files must be regular files.
Device/inode/size/mtime/ctime snapshots detect common replacement and modification
races; this does not promise immutable filesystem snapshots or hard-link privacy.
No source file is renamed, modified, or read solely to mutate traffic counters.

State/config/receiver writes are absent from this first slice. Self-test writes
only a small explicitly created temporary fixture, after a disk-capacity check,
and removes that fixture on normal exit. Before every repository write,
dependency-resolution, build, artifact or download operation, evaluate all
affected filesystems and peak temporary/cache/build growth. Preserve at least
max(1 GiB, 5% capacity). Unknown capacity/estimate fails closed. Do not clean
unrelated user directories, caches or running applications to force a pass.
Heavy compilation, tests and packaging run only on disposable CI runners.

## Alternatives not taken

A line-by-line Python port would reproduce queue/thread/global-state defects.
A full asynchronous regular-file reader would still run blocking filesystem work
somewhere; explicit ownership and bounded queue are easier to audit here.
Inventing a generic distributed control plane or 13 empty packages would add
abstraction without demonstrated behavior. Claiming complete migration from this
slice would conceal unresolved required capabilities and benchmark gates.
