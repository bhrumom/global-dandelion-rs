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

The reviewed baseline helper and same-host cutover harness now exist, but their
successful execution is only candidate evidence until it is bound to an exact-head
artifact and independently reviewed. Remaining external acceptance still includes
representative physical Wi-Fi hotspot and storage hotplug/replacement proof,
production release trust, and controlled-cutover review. Longer-duration
backpressure/fault-injection measurements remain useful engineering evidence but do
not replace those five formal gates. The engineering targets in the Spec are targets,
not measured achievements.

## Equivalent legacy packet comparison

CI now runs scripts/python_comparison.py on the same native runner for Python and
Rust. It uses the exact reviewed struct.pack + MD5 + payload expression, alternating
measurement order, identical payload and packet counts, and matching final packet
hashes. Raw repetitions include encoding elapsed time and GNU-time CPU/peak RSS.
The report is checksummed in each artifact. It is not a full scanner/network/drive
cutover comparison; shared-runner values and 0.01-second CPU resolution are explicit.
No side-effectful Python module is imported or run.


## Controlled same-host cutover harness

`scripts/controlled_cutover.py` is the operator-facing harness for the
`controlled-cutover` external gate. It verifies the exact pinned Python Git blob,
never imports or executes the legacy module's normal main function, and reproduces
only reviewed side-effect-free scan/read/legacy-loopback expressions on synthetic
local fixtures. It alternates Python/Rust order and records raw wall time, user/system
CPU, peak RSS, dataset throughput and repeated SIGTERM settlement distributions.

The harness requires `--tested-commit`, `--artifact-head-commit`, and
`--artifact-build-info`. The tested commit and workflow head must be the same full
SHA, and the harness independently reads `BUILD-INFO.txt` from the extracted artifact
and requires its `git_sha` to match as well. Candidate evidence therefore fails closed
if a pull-request workflow metadata SHA differs from the commit actually checked out
and compiled. Wire contracts are reported separately:
the Python baseline uses the pinned u32be+MD5 UDP packet expression while Rust uses
protocol v2 and verified local receipts. That difference must not be presented as
identical wire work or as a universal Rust/Python speedup claim.

A successful harness run is only **candidate gate evidence**. The JSON report still
must satisfy the repository evidence contract for the exact final commit and be
validated by a distinct independent reviewer through the external evidence manifest.
Smoke runs on an older artifact, shared CI packet benchmarks, or self-review never
make `controlled-cutover` pass.
