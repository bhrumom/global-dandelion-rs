# Complete migration implementation record

Spec revision 2 replaces revision 1. Baseline: 7a0f78d; green run: 36421256894.
Requested outcome: all required legacy domains, not only the initial slice.

Current increment: durable state/import and platform inventory/watch, followed by
runtime integration and exact-revision CI. Verification records will identify actual
code and runs; unimplemented domains and hardware/performance gates remain open.

## Implemented production increment

- C15/C22: private-directory writer lock, checksummed versioned state, sync/rename/
  directory-sync commits, crash-conservative whole-run traffic leases, explicit
  idempotent import of the REAL Python traffic_state.json schema (current_month,
  total_sent_bytes). No monthly automatic reset, and byte/file resume remains open.
- C06/C07/C17: bounded mountinfo/proc/sys observation, passive TCP/ARP/interface
  reporting, mount identity differences and explicitly allowed mount namespaces.
  Runtime enrolls only mount roots within that policy. Hardware hotplug still needs
  real-device evidence; no new mount is an authorized network destination.
- C04/C05: real scan metadata/read commands; metadata never reads payload, read
  verifies selected file bytes through the pipeline without opening sockets.
- C13: day/night schedule with explicit UTC offset, positive validated rates and
  monotonic waits; deterministic boundary tests. No host time mutation.
- C19: latest-value terminal observer on an owned worker, 4 Hz refresh, terminal
  restoration guard, non-TTY rejection; headless output remains separate.
- C01: repeat/watch service conserves one budget across passes and cancellation;
  binary main reduced to parsing/log initialization/error mapping.

Tests and CI must be validated on the new exact revision before reporting success.

CI run 36426448387: dependency audit passed; native jobs stopped at a Clippy
collapsible-if lint in the new state adapter. Repairing the code without weakening
the lint gate; behavioral tests have not yet been claimed for this increment.
