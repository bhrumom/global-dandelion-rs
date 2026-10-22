# Mac resume mount identity investigation

The Apple adapter intentionally rejects strict resume: a stable device/volume UUID
is not a mount generation. GD-MAC06/AP-05 and ADR0007 remain outstanding.

Before choosing an implementation, run a bounded disposable Actions experiment:
create one APFS image, write one source, query statfs identity through a native
helper, unmount and remount the same image at the same path across independent
process invocations. Preserve file stamps and volume UUID as controls. An unchanged
f_fsid falsifies the claim that statfs alone provides a remount generation. A changed
f_fsid on this image is only filesystem-specific experimental evidence, never a
general Mac resume approval. No production resume gate is relaxed by this probe.

Apple documents vfs_getnewfsid as generating a unique filesystem ID for a mount,
typically called by filesystem mount code. It does not promise every filesystem
changes identity on every remount. Source:
https://developer.apple.com/documentation/kernel/1523259-vfs_getnewfsid

The probe must execute only on a disposable macos-15 Actions runner, preserve disk
reserve, use a task-owned image/mountpoint and detach that exact image in finally.
No physical device, user source, privileged global mount or production state is
modified. Record raw observations even when the candidate identity fails.

## FSEvents follow-up, 2026-10-21

Actions39434309650 falsified statfs-only identity: all three APFS remounts returned
the same fsid with unchanged file stamps. Next inspect per-device FSEvents history
across independent processes: capture a cursor/daemon UUID, read an unchanged
control, remount while no observer runs, then replay history. Record HistoryDone,
Mount/Unmount/RootChanged, missing/coalesced/dropped history, cursor bounds and
UUIDs. Bounded history replay is an experiment, not a production proof. Missing
events, ambiguous history or stream failure must never be interpreted as an
unchanged mount. The installed0.4.0 package and resume rejection remain unchanged.

The native observation helper is test-only. It is compiled only on disposable
Actions, returns bounded numeric metadata and UUIDs, never grants source access
or accepts a synthetic proof from a WebView. Apple describes persistent streams
as advisory, recommends per-device streams, and requires a rescan on dropped,
wrapped or root-change events:
https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/UsingtheFSEventsFramework/UsingtheFSEventsFramework.html

Actions39522936806 found the test image had no daemon UUID and its per-device
replay returned HistoryDone without the real remount events. HistoryDone alone is
therefore insufficient. Keep that negative result. A second instrument watches
the task-owned parent directory on the host's data filesystem, recording the
parent daemon UUID and history controls separately. It tests whether mountpoint
changes are visible in the containing filesystem's history, without treating the
external volume's advisory content log as a mount-generation authority.

Actions39523154205 (source20a85cf) also falsified the containing-filesystem history
as currently proposed: its UUID remained present, both real remounts completed,
and historical replay returned HistoryDone, but no Mount/Unmount flags appeared.
The unchanged control reported only HistoryDone. Other file changes in the parent
were observable; they do not prove a mount generation. Therefore neither tested
history variant is sufficient to authorize reuse of a durable checkpoint.

Retained evidence: Actions39434309650 (statfs),39522936806 (per-device log),
39523154205 (parent history). These are completed negative design experiments.
They change no production capability, package version, Host or installed engine.
The next implementation must supply a source lifetime/generation authority and
test its interruption/remount/history-loss controls. Do not enable Mac resume by
merely removing the platform guard, accepting HistoryDone, or trusting file/volume
UUID equality. Full cross-restart parity remains unimplemented.
