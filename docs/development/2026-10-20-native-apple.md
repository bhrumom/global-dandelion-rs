# Apple core implementation and recovery

Authority: `docs/specs/native-apple-core.md`, AP-01..07; source base
`0fc0c7513ddac495a7149038583670c29cbe56fa`; PR #7. User requested full
Mini App functionality on other platforms. This source increment does not settle
the remaining Windows/mobile/native inventory/hardware/installed Host requirements.

Native Apple run 39398956110 at `36261f06a57db0820d412fc8e6fa8be9d36cce7d`
passed compilation, Clippy, CLI tests, platform adapter tests including held-root
rename/nested symlink, and most workspace tests. Scanner fixture creation failed
with APFS EILSEQ (92), before scanning. Classification: TEST_FLAW. The invalid-byte
fixture remains on filesystems that support it; Apple EILSEQ alone selects a real
Unicode-name fixture. All scanner payload/cancellation/source mutation assertions
remain. Other creation errors remain failures. No arbitrary rerun or skip.

Added FIFO nonblocking rejection and opened-handle mutation stamp checks. Linux
openat2 and Apple openat/NOFOLLOW require the same assertions. Exact final source
Actions results and independent full-parity gates remain distinct.
