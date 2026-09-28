# Python -> Rust migration and acceptance ledger

Authoritative source: `bhrum/global@aa7f3720c42856f0e1d18742f78fba156227ee79`,
blob `54bf6226eb80d1ef6c4f6087fb02579141ab134f`.

This inventory preserves all 37 original responsibility rows. `implemented` means
production code and real test paths exist; it is NOT full hardware/cutover sign-off.
The required physical, release-trust and controlled-baseline evidence is tracked
separately in [acceptance-gates.json](../acceptance-gates.json). Inspect exact-HEAD CI.
A green foundation gate must never be described as unconditional full replacement.

| Responsibility | Classification | Code status | Verification scope | Destination |
|---|---|---|---|---|
| time-metadata | redesigned | implemented | current-ci | platform time / authenticated peer control |
| host-clock-mutation | intentionally-removed | removed | documented-exclusion | none |
| disk-partitions | redesigned | implemented | current-ci | platform-linux inventory |
| disk-space | redesigned | implemented | current-ci | platform-linux |
| network-interfaces | redesigned | implemented | current-ci | platform-linux inventory |
| connections | redesigned | implemented | current-ci | platform-linux inventory |
| structured-logging | redesigned | implemented | current-ci | app |
| progress-ui | redesigned | implemented | current-ci | dandelion-tui |
| live-speed-stats | redesigned | implemented | current-ci | core telemetry + tui |
| day-night-pacing | redesigned | implemented | current-ci | core schedule + pacing |
| constant-rate | redesigned | implemented | current-ci | pacing |
| surface-scan | redesigned | implemented | current-ci | scanner + ReadVerifier |
| metadata-scan | redesigned | implemented | current-ci | scanner metadata mode |
| hotspot | redesigned | implemented | physical-hotspot-required | hotspot + Linux ProcessRunner |
| release-version | redesigned | implemented | current-ci | dandelion-updater |
| updater | redesigned | implemented | production-trust-required | signed updater + opt-in systemd timer |
| rename-effect | legacy-adapter | implemented | current-ci | platform-linux legacy link capability |
| loopback-send | redesigned | implemented | current-ci | transport |
| loopback-receive | redesigned | implemented | current-ci | transport |
| private-vps | redesigned | implemented | current-ci | mutual TLS transport |
| lan | redesigned | implemented | current-ci | mutual TLS transport |
| startup-drive-scan | redesigned | implemented | physical-hotplug-required | explicit mount policy + service |
| external-drive-monitor | redesigned | implemented | physical-hotplug-required | mount diff observer + service |
| source-scan | redesigned | implemented | current-ci | scanner |
| chunk-framing | redesigned | implemented | current-ci | packet |
| wan-workers | redesigned | implemented | current-ci | mutual TLS transport |
| worker-explosion | intentionally-removed | removed | documented-exclusion | Supervisor |
| shutdown | redesigned | implemented | current-ci | Supervisor |
| state | redesigned | implemented | current-ci | dandelion-state |
| country-metadata | redesigned | implemented | current-ci | dandelion-compat country |
| random-public-targets | intentionally-removed | removed | documented-exclusion | none |
| target-health | redesigned | implemented | current-ci | transport peer receipts / socket meter |
| neighbour-discovery | redesigned | implemented | current-ci | read-only platform inventory |
| target-policy | redesigned | implemented | current-ci | core exact-address capability |
| batch-send | redesigned | implemented | controlled-cutover-required | bounded authenticated peer fanout |
| wire-budget | redesigned | implemented | current-ci | core + Supervisor |
| orchestration | redesigned | implemented | current-ci | app |

[Machine-readable inventory](parity.json) contains original symbols, ranges, reasons and concrete evidence paths. Three unsafe legacy defaults remain deliberately removed. No pending domain is replaced by a no-op command. Some redesigned behaviors intentionally differ: lifetime rather than automatic monthly budgets, authenticated unicast rather than unsolicited broadcasts, file-level rather than byte-level resume, and signed staging rather than active binary self-overwrite.
