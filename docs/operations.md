# Configuration, service operation and recovery

## Before running

Use an unprivileged account with read-only access to selected source directories.
Defaults: 32 queued chunks, 1,152-byte payload, 1 MiB/s target rate, 64 MiB per-run
traffic budget, 256 MiB source-read budget and 3-second cooperative shutdown.
`--watch` does not remove finite budgets. Runtime memory includes more than queued
payloads; observe real RSS rather than interpreting queue limits as total RSS.

Configuration order: defaults < explicit TOML < DANDELION__SECTION__KEY < CLI.
`DANDELION_CONFIG` supplies a default path. No implicit current-directory config
search. Unknown fields and invalid limits fail before starting runtime components.
Environment values use TOML syntax; paths use native OS strings internally.

```sh
export DANDELION__RUNTIME__QUEUE_CAPACITY=16
export DANDELION__PACING__BYTES_PER_SECOND=524288
dandelion --config examples/service.toml config check
dandelion --config examples/tls-client.toml run --dry-run --json
```

Logs go to stderr, results to stdout. `--json` controls result encoding;
`--log-format json` controls tracing. Keys/password contents are never config-dumped.

## Authorized peers and receiver

Provision a private CA and separate server/client identities with correct SAN and
extended key usage; never use CI fixture keys. Client private keys require private
file permissions and regular/no-symlink/no-hardlink inputs. Explicit numeric peer
addresses are checked independently from certificate DNS names. Trust is not
inferred from ARP tables, country metadata or newly mounted volumes.

The receiver uses mTLS for all sessions. Non-loopback binds require explicit remote
consent and allowed client IPs; keep CA scope narrow and rotate certificates through
an operator-controlled policy. No plaintext payload fallback is supported.

Receive output is a JSON ready event followed by a final report. Frames and bytes
in the final receive report describe completed sessions; failed sessions are counted
separately and must not be interpreted as successful durable transfer. Receiver
session tables, record sizes, timeouts and ciphertext budgets are bounded.

## Durability and file-level recovery

Set state.enabled=true and use a private directory. Service mode should use
`/var/lib/dandelion/runtime` with an operator-provisioned service account. The
writer lock remains on a stable inode while state files are replaced atomically.

The budget is leased before network activity, then settled after owned tasks stop.
An error conservatively retains the whole lease, so a crash cannot reset quota.
An unexpectedly exhausted budget may therefore include uncertain work from a
prior failure. Do not delete state to bypass it. Inspect and preserve history before
an explicit operator recovery. There is no automatic monthly reset.

Legacy import accepts the actual Python traffic_state.json format:
`{"current_month":"2026-09","total_sent_bytes":1234}`. It is bounded, idempotent and
updates a conservative lifetime floor, not an additive re-import charge. It never
authorizes peers or changes source roots. Unknown fields/negative counters fail.

`run --resume` retains at most 4096 file-level receipt keys. Changing source metadata,
policy or certificates invalidates matching checkpoints. Incomplete files are read
from the start; abrupt termination can replay acknowledgements not yet committed.
This is appropriate for memory-verification receivers, not exactly-once durable
storage. Full byte-resume or remote file synchronization is not claimed.

## Mounts, scans and clocks

`platform inspect` observes mount, interface, TCP and IPv4 ARP data; warnings identify
missing/incomplete proc/sys views. `platform watch` reports bounded mount differences.
Configured non-root mount prefixes explicitly opt in to source enrollment between
service passes. An unmount during a file operation fails closed. Real unplug/reinsert
recovery behavior remains an acceptance task on representative hardware.

`scan metadata` never opens payload files. `scan read` verifies selected file data
without sockets. Both honor bounds and cancellation. The normal data engine never
renames sources or mutates system time/network settings.

`platform time` is passive local metadata. `platform time --peer` compares configured
authenticated peer clocks with a monotonic round-trip estimate, using the same
ciphertext budgets and optional durable lease. It reports uncertainty and never
modifies host clocks or claims the peer is an accurate UTC authority.

## Explicit hotspot operations

```sh
dandelion hotspot plan --interface wlan0 --ssid Global-Dandelion --band a
dandelion hotspot status --journal-directory /var/lib/dandelion/hotspot
```

Applying a hotspot can replace the interface's current connection. Review the plan
and network access consequences first. Apply/stop require --confirm; apply also needs
a private password file and existing NetworkManager authorization. There is no
hidden sudo, interactive secret prompt or automatic service startup. The journal
records owned UUID and previous connection before changes. Rollback never modifies
an unrelated profile or overrides a later operator-selected connection. If rollback
fails, the journal remains marked rollback-required for explicit stop/recovery.

Private temporary password input is removed on normal completion; an abrupt host
crash may leave a private temporary file. Protect the directory and review recovery.
Mock process tests do not certify a particular Wi-Fi driver or physical radio.

## Signed updates and periodic discovery

```sh
dandelion update check --manifest release.json --signature release.sig --public-key trusted-key.raw
dandelion update stage --manifest release.json --signature release.sig --public-key trusted-key.raw \
  --artifact downloaded-artifact --directory /var/lib/dandelion/staging
```

The public key is a separately trusted raw Ed25519 key (32 bytes); detached signatures
are 64 bytes over exact manifest bytes. HTTP metadata/artifact content is untrusted
until signature and digest validation. HTTPS metadata and signature URLs are supported;
artifact downloads require an explicit allowed hostname. Redirects/proxies/URL
credentials are not silently followed. Never trust a key simply because it arrived
next to an artifact. Production key provisioning is a distinct operator task.

Staging uses a private temporary bundle and atomic no-overwrite promotion. It does
not execute or replace the active binary. A service manager/operator selects a
validated version and performs activation/rollback. `--allow-rollback` must be
explicit when staging an older stable version. Same/older releases are rejected
without that flag; a periodic check can therefore report a non-success when no
new acceptable release is present rather than install the same version repeatedly.

The optional systemd check service/timer uses operator-defined MANIFEST_URL,
SIGNATURE_URL and PUBLIC_KEY_FILE in /etc/dandelion/update-check.conf. The service
account, paths, CA permissions, key trust and sandbox must be validated at deployment.
Templates are not installed/enabled by this CLI or by CI. Normal transfer service
restarts do not reset quotas; budget/config/verification exits are not auto-restarted.

## Legacy compatibility

`compat packet-inspect` handles the original u32be+MD5 framing only as explicit
legacy interoperability. It is not cryptographic authentication. Country CSV lookup
requires geoname_id/country_name and network/geoname_id headers, is size/row-bounded,
and never grants target authorization. Supply appropriately licensed local datasets.

`compat link-exercise --confirm` operates on hard/symbolic links in an owned private
workspace; original contents/name are not changed, but inode metadata is. Excluded
source-code/config/credential/hidden paths must not be used. Use a quiescent trusted
source parent. `compat link-recover --confirm` verifies the original identity and
only touches known journal slots. Missing original identity preserves the remaining
link; no heuristic filename restoration or deletion of unrelated files occurs.

## Exit codes and acceptance

0 success; 1 operational/filesystem/state failure; 2 configuration/arguments;
4 protocol/verification; 75 budget exhausted; 130 cancellation. Errors do not become
success merely because stdout is closed. SIGINT/SIGTERM share supervised cleanup;
kernel-blocked filesystem calls remain an explicit timeout limitation.

Artifacts contain checksums, provenance metadata, original dependency notices,
license inventory, actual CLI acceptance JSON and raw benchmarks. Validate the
archive checksum, extract, verify inner SHA256SUMS and execute self-test. These
are GNU/glibc Linux CI artifacts, not a signed production release guarantee.

`scripts/check_parity.py` validates code inventory. `--require-complete` additionally
requires an external reviewed evidence manifest for the exact checked-out commit,
covering physical hardware, production trust and controlled whole-program cutover.
A mocked test, unsigned metadata list or old green run is not sufficient evidence.
