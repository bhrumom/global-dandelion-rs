# ADR 0002: invert source filesystem capabilities

Status: accepted. Date: 2026-09-28. Refines ADR 0001's initial scanner/platform edge.

The core defines SourceAccess, SourceRoot and SourceFile ports, plus an opaque
handle-based FileStamp. They depend only on standard Rust I/O/path types and the
core error model. The scanner depends on these contracts, not the Linux adapter.
The app is the composition root and explicitly supplies LinuxSourceAccess.

The Linux adapter owns openat2 path policy and dev/inode/mtime/ctime identity.
Metadata is sampled from the same file handle before/after reading, eliminating
a second path-based metadata lookup that might refer to a different file after a
rename. Paths authorize a rooted read; file ids remain transfer-local values.

The scanner's tests inject the same real Linux adapter as production as a dev-only
dependency. Kernel policy tests remain in the Linux adapter. Static checking
rejects a production scanner -> platform dependency. Future platforms must supply
an equally safe adapter, not weaken the Linux implementation through fallback.

This change does not add additional feature domains or claim all platforms work;
it strengthens the first slice's dependency inversion and testability.
