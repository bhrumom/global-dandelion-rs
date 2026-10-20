# ADR 0011: extend the existing native filesystem owner for Apple

The existing platform adapter owns SourceAccess, file stamps, credentials and disk
reserve. Extend it with Apple component-wise openat/NOFOLLOW rather than introducing
a parallel scanner, using racy canonicalize-then-open, or weakening Linux openat2.
The compatibility package name remains unchanged; composition uses NativeSourceAccess.
No unsafe Rust or new dependency is required. Apple and Linux both use opened-handle
identity, bounded queues, the same transport, pacing, state and updater owners.

Linux mount generation/proc/sys inventory and NetworkManager do not become Apple
implementations through a cfg flag. Those operations reject explicitly. Real native
mount identity/resume, networking inventory and reversible hotspot adapters remain
unimplemented Apple requirements. Windows and mobile Host integration remain separate
work. See AP-01..07 in `../specs/native-apple-core.md` for verification and release.
