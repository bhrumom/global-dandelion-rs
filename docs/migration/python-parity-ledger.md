# Python -> Rust Parity Ledger

Source: `bhrum/global@aa7f3720c42856f0e1d18742f78fba156227ee79`, file blob
`54bf6226eb80d1ef6c4f6087fb02579141ab134f`.

Canonical machine-readable inventory: [parity.json](parity.json). Classification
and implementation status are separate. `implemented` means code and test paths
exist; it is **not** a statement that full parity/performance/cutover is verified.
Inspect the exact CI revision. Pending capabilities are not replaced by no-ops.

| Responsibility | Python symbols | Classification | Status | Destination |
| --- | --- | --- | --- | --- |
| time-metadata | `get_current_local_time`, `sync_network_time` | redesigned | **pending** | future telemetry |
| host-clock-mutation | `sync_network_time` | intentionally-removed | **removed** | none |
| disk-partitions | `DiskPartition`, `linux_get_disk_partitions` | redesigned | **pending** | platform-linux |
| disk-space | `DiskUsage`, `linux_get_disk_usage` | redesigned | **implemented** | platform-linux |
| network-interfaces | `linux_get_net_interfaces`, `find_specific_interfaces` | redesigned | **pending** | platform-linux |
| connections | `linux_get_net_connections`, `analyze_network_connections` | redesigned | **pending** | platform-linux |
| structured-logging | `log_message`, `HeadlessLogPrinter` | redesigned | **implemented** | app |
| progress-ui | `create_progress_bar`, `StatusDisplayThread` | redesigned | **pending** | future tui |
| live-speed-stats | `update_speed_stats`, `format_speed`, `_get_instantaneous_speed`, `get_worker_speed`, `get_category_speeds`, `cleanup_inactive_speed_stats` | redesigned | **pending** | future telemetry |
| day-night-pacing | `_apply_speed_level`, `TimeBasedPacerThread` | redesigned | **pending** | pacing |
| constant-rate | `_apply_speed_level`, `send_chunk_in_batches` | redesigned | **implemented** | pacing |
| surface-scan | `ThrottledSurfaceScanThread` | redesigned | **pending** | future compatibility scanner |
| metadata-scan | `MetadataScanThread` | redesigned | **pending** | scanner |
| hotspot | `HotspotDataBroadcaster` | redesigned | **pending** | future hotspot capability |
| release-version | `_parse_version` | redesigned | **pending** | future updater |
| updater | `AutoUpdateThread` | redesigned | **pending** | future signed updater |
| rename-effect | `RenamerThread`, `restore_interrupted_files` | redesigned | **pending** | future optional journaled compatibility |
| loopback-send | `LoopbackSenderThread` | redesigned | **implemented** | transport |
| loopback-receive | `LoopbackReceiverThread` | redesigned | **implemented** | transport |
| private-vps | `VpsSenderThread` | redesigned | **pending** | future authorized transport |
| lan | `ForcedNetworkScanThread` | redesigned | **pending** | future authorized LAN transport |
| startup-drive-scan | `StartupDriveScanner` | redesigned | **pending** | future drive observer |
| external-drive-monitor | `ExternalDriveMonitorThread` | redesigned | **pending** | future drive observer |
| source-scan | `FileScannerThread` | redesigned | **implemented** | scanner |
| chunk-framing | `FileScannerThread.process_file` | redesigned | **implemented** | packet |
| wan-workers | `ChunkSenderWorker` | redesigned | **pending** | future authorized transport |
| worker-explosion | `WorkerManagerThread` | intentionally-removed | **removed** | Supervisor |
| shutdown | `graceful_shutdown` | redesigned | **implemented** | Supervisor |
| state | `load_state`, `save_state` | redesigned | **pending** | future state |
| country-metadata | `load_country_database` | redesigned | **pending** | future optional metadata importer |
| random-public-targets | `generate_ip_from_cidr`, `prepare_target_ip_pool` | intentionally-removed | **removed** | none |
| target-health | `check_internet_connectivity` | redesigned | **pending** | future transport health |
| neighbour-discovery | `scan_network_devices` | redesigned | **pending** | future read-only observer |
| target-policy | `prepare_target_ip_pool` | redesigned | **pending** | future config/policy |
| batch-send | `send_chunk_in_batches` | redesigned | **pending** | future optimized transport |
| wire-budget | `ChunkSenderWorker` | redesigned | **implemented** | core + Supervisor |
| orchestration | `main` | redesigned | **implemented** | app |

Each JSON row records source ranges, reasons and concrete implementation/test evidence paths. Performance-sensitive rows additionally require controlled baseline measurements before cutover; these are still pending. The original full acceptance criteria in the Spec remain authoritative.
