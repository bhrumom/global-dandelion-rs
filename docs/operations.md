# CLI configuration and operations

## Configuration order

Compiled defaults < explicit TOML file < `DANDELION__SECTION__KEY` variables
< provided CLI flags. `--config FILE` selects a file; `DANDELION_CONFIG` supplies
its default. The CLI does not silently search working directories for config.
A partial TOML file merges with defaults, so unspecified fields stay documented.
Unknown fields, zero limits, oversized chunks/queues or unsupported transport
modes are errors before scanning or opening sockets. Config input is capped at
1 MiB. Source paths may be non-UTF-8 through CLI; TOML/JSON serialization of such
paths can fail and is not lossy-silently substituted.

```sh
export DANDELION__RUNTIME__QUEUE_CAPACITY=16
export DANDELION__PACING__BYTES_PER_SECOND=524288
export DANDELION__SCANNER__ROOTS='["/srv/dandelion-data"]'
dandelion config check
dandelion run --rate-bytes-per-second 1048576 --dry-run --json
```

Environment setting values use TOML syntax, including quoting string values and
using arrays for roots. `DANDELION_LOG` controls tracing filters. Logs always go
to stderr, data/results to stdout. `--log-format json` makes logs structured;
`--json` makes command results machine-readable. `--help`, `--version`, shell
completions, headless execution and broken stdout pipes are supported.

## Exit codes

| Code | Meaning |
| ---: | --- |
| 0 | Completed successfully, validated/dry-run, or output pipe closed |
| 1 | I/O, source mutation, scanner/task/disk error, or shutdown timeout |
| 2 | Invalid configuration or command line |
| 4 | Packet/local receive verification failure |
| 75 | Global wire-traffic budget exhausted before the next full datagram |
| 130 | SIGINT/SIGTERM or explicit cancellation |

Attempted wire bytes, accepted wire bytes and verified payload bytes are distinct.
The budget includes the 80-byte header on every packet and is never exceeded by
partially admitting a datagram. No unlimited default or automatic retry exists.
A failure after kernel send can make a packet's final receipt unknown; the CLI
returns an error, not a successful end-to-end receipt. All totals are per run;
persistent/lifetime accounting is not implemented yet.

## Service operation

Run as an unprivileged account with read-only access to selected data directories.
`run` performs one pass. A `systemd` Type=oneshot service or an external scheduler
may invoke it; no daemon/service installation or host configuration is performed.
The existing Python daemon should not be retired until the full parity and
benchmark acceptance gates in the Spec are met. Do not operate this as an
unbounded automatically restarting traffic generator.

## Troubleshooting

`run --dry-run --json` validates parameters without source access or sockets.
`config check` validates configuration, but does not certify source accessibility.
`doctor` reports disk capacity and the platform, without external network probes.
`self-test --json` verifies a 65,537-byte file plus an empty file over actual UDP.

An `openat2` failure on an old kernel or a denied sandbox is a deliberate fail-closed
behavior. Use Linux 5.6+ with the capability available; do not substitute unsafe
symlink-following access. A nested mounted volume needs its own explicitly chosen
source root. A source-read budget stops a run rather than silently skipping large
files. Reconfigure it deliberately or select a smaller source tree.

For CI artifacts, check the exact commit SHA and successful required gate, then
verify `SHA256SUMS`. Upgrades/rollback currently mean selecting another validated
artifact, not allowing the running process to overwrite itself. No signed-release
trust claim is made by checksum metadata alone.
