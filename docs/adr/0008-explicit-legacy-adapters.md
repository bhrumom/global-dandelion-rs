# ADR 0008: explicit legacy adapters

Status: implemented pending exact-revision verification. Spec C22-C24.

Original RenamerThread creates and renames hidden hard links and symlinks, not the
original file. The optional command confines that behavior to four fixed slots in
a private workspace with a durable source-identity journal and no-overwrite
renames. Recovery only acts on those owned slots. Missing or replaced originals
cause refusal and preservation of the remaining hard link. Original filename and
contents are unchanged; inode ctime and link count necessarily change. The selected
file and parent must be quiescent and operator-controlled. Never use the effect
concurrently with ordinary source scanning.

The old filename-pattern restoration/deletion heuristic is not reproduced. No
unknown files are guessed, deleted or overwritten. A small retained ownership
journal supports audit and explicit recovery after interruption.

Country lookup is bounded, header-based CSV metadata for one requested address.
It never generates addresses, authorizes peers or downloads licensed databases.
Legacy MD5 packets are explicit inspection/benchmark adapters, not secure transfer
frames. The production transfer format remains v2 inside authenticated TLS.
