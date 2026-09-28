# ADR 0007: bounded, explicitly file-level resume

Status: implemented pending exact-revision CI. Spec C16.

The core checkpoint port separates scanner identity from receiver acknowledgements.
The app owns a bounded 4096-file checkpoint index and persists it with the state
store after all runtime work settles. It hashes the exact target policy and TLS
trust/client certificate bytes into a scope; source keys include canonical root,
relative OS path and the same opened file handle's size/device/inode/mtime/ctime
stamp. Changed policy/certificates/content metadata invalidate prior completion.
Only terminal chunks acknowledged by every selected peer mark a file complete.

`run --resume` requires durable state and a single finite pass. A subsequent run
skips acknowledged unchanged files before payload reads. Incomplete files restart
from byte zero. There is no silent eviction or unbounded file index; partition
workloads larger than 4096 candidate files. The source-access and checkpoint ports
are wired by the composition root rather than concrete scanner-to-state coupling.

The checkpoint currently commits at controlled shutdown/run settlement, not each
packet. A SIGKILL/host crash can replay files acknowledged since the last durable
checkpoint. This is conservative at-least-once in-memory verification, NOT byte-level
resume, exactly-once remote file storage or an immutable filesystem snapshot.
Outstanding traffic leases remain charged on error/crash, so replay cannot reset
the lifetime budget. Tests cover acknowledgement, policy invalidation, reload,
unchanged-file skip and changed-file resend through the actual CLI.
