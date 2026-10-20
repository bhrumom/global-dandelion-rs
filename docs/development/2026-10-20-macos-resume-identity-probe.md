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
