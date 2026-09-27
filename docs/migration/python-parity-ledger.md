# Python -> Rust Parity Ledger

Baseline: `bhrum/global@aa7f3720c42856f0e1d18742f78fba156227ee79`

This ledger is the migration evidence index. Status values:

- `planned`
- `mapped`
- `redesigned`
- `legacy-adapter`
- `verified`
- `intentionally-removed`
- `not-applicable`

A row may move to `verified` only when implementation and test/benchmark evidence are linked.

| Python responsibility | Python symbol / area | Rust destination | Status | Evidence |
|---|---|---|---|---|
| network time metadata | `sync_network_time` | platform/config telemetry; no implicit host clock mutation | redesigned | pending |
| disk partitions | `linux_get_disk_partitions` | `dandelion-platform-linux` | planned | pending |
| disk usage | `linux_get_disk_usage` | `dandelion-platform-linux` | planned | pending |
| network interfaces | `linux_get_net_interfaces` | `dandelion-platform-linux` | planned | pending |
| connection summary | `linux_get_net_connections` | `dandelion-platform-linux` | planned | pending |
| speed statistics | `update_speed_stats` and helpers | `dandelion-telemetry` | planned | pending |
| time-based pacing | `TimeBasedPacerThread` | `dandelion-pacing` | planned | pending |
| throttled surface scan | `ThrottledSurfaceScanThread` | scanner compatibility module | planned | pending |
| metadata scan | `MetadataScanThread` | `dandelion-scanner` | planned | pending |
| hotspot broadcast/control | `HotspotDataBroadcaster` | `dandelion-hotspot` + Linux adapter | planned | pending |
| updater | `AutoUpdateThread` | `dandelion-updater` | redesigned | pending |
| rename effect | `RenamerThread` | optional compatibility component with journal | redesigned | pending |
| loopback sender | `LoopbackSenderThread` | loopback transport | planned | pending |
| loopback receiver | `LoopbackReceiverThread` | verifier/loopback transport | planned | pending |
| configured VPS sender | `VpsSenderThread` | private/VPS transport | planned | pending |
| LAN broadcaster | `ForcedNetworkScanThread` | LAN transport | redesigned | pending |
| headless logging | `HeadlessLogPrinter` | tracing subscriber | redesigned | pending |
| status display | `StatusDisplayThread` | `dandelion-tui` | planned | pending |
| startup drive scan | `StartupDriveScanner` | `dandelion-scanner` + Linux adapter | planned | pending |
| external drive monitor | `ExternalDriveMonitorThread` | Linux drive observer | planned | pending |
| file scan/chunk production | `FileScannerThread` | scanner + bounded chunk pipeline | redesigned | pending |
| WAN worker pool | `ChunkSenderWorker` | transport tasks | redesigned | pending |
| dynamic worker manager | `WorkerManagerThread` | supervisor/pacing concurrency permits | intentionally-removed | pending |
| persistent state | `load_state/save_state` | `dandelion-state` | planned | pending |
| country CSV database | `load_country_database` | optional target metadata importer | planned | pending |
| random CIDR target generation | `generate_ip_from_cidr` use for unsolicited WAN targets | none as default behavior | intentionally-removed | pending |
| internet connectivity check | `check_internet_connectivity` | target-aware health checks | redesigned | pending |
| neighbour discovery | `scan_network_devices` | Linux observer; not auto-targeting | redesigned | pending |
| target pool preparation | `prepare_target_ip_pool` | validated explicit target policy | redesigned | pending |
| per-packet WAN send loop | `send_chunk_in_batches` | authorized transport + pacing + optional batching | redesigned | pending |
| interrupted rename restore | `restore_interrupted_files` | compatibility journal recovery | planned | pending |
| process orchestration | `main` | top-level Supervisor | redesigned | pending |

## Evidence rules

For every verified row, add links or commit SHAs for:

1. implementation;
2. unit/integration/parity test;
3. benchmark when the row is performance-sensitive;
4. ADR when behavior intentionally changes architecture or protocol.

The ledger is expected to grow more granular as implementation proceeds.
