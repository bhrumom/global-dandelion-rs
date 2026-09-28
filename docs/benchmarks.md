# Measurement plan and limitations

No measured Rust/Python speedup is claimed in this implementation.
The x86_64 quality job also runs a short controlled encode/decode benchmark and
includes raw Criterion estimates in `packet-benchmark.json` inside its checksummed
artifact. These shared-runner values are inspectable evidence, not stable hardware
performance limits or proof of improvement over Python.

The existing source is pinned to commit
`aa7f3720c42856f0e1d18742f78fba156227ee79`, blob
`54bf6226eb80d1ef6c4f6087fb02579141ab134f`.

`Controlled local benchmark` compiles/runs Criterion packet encode/decode on a
disposable GitHub Actions runner and saves raw results. `Rust CLI Quality` verifies
actual local UDP, exact byte totals, source immutability and process SIGTERM under
a stalled consumer/rate limiter. It captures one RSS sample under backpressure;
this is a regression guard, not a universal RSS upper bound or long-duration soak.

Before a full cutover, run equivalent controlled workloads on identical hardware:
small-file/many-directory scanning, large sequential reads, legacy MD5 framing,
packet v2 checksum framing, stop-and-verify UDP and any optimized authorized
transport. Record payload/wire bytes separately, elapsed wall time, user/system
CPU time, peak RSS, queue limits, kernel/toolchain/CPU details, workload manifest,
repetition count, percentiles and confidence intervals.

Do not execute the Python script's normal main function in CI: it includes broad
network and host-mutating behavior. Extract a reviewed pure baseline helper or
use a quarantined benchmark harness limited to synthetic local fixtures. Label
wire-format/verification differences rather than treating them as identical work.
The new protocol's larger metadata/checksum is not the legacy MD5 contract.

Remaining acceptance: baseline helper and parity workload comparison, repeated
long-duration backpressure trials, mutation/drive-unplug fault injection, verified
external transport, and hardware-controlled CPU/GiB and throughput assessment.
The engineering targets in the Spec are targets, not measured achievements.
