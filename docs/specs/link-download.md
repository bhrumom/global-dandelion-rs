# Link download in the Mini App composer

LD01: pasting public HTTP(S) links starts a bounded package-owned Rust download.
File URLs download the actual body. HTML pages are saved with one level of public
images, audio/video sources, OpenGraph media and explicit/downloadable attachments.
No page scripts, login bypass, cookies, recursive crawling or automatic payload
transmission. A normal send uses the downloaded bytes, not just the URL string.

LD02: at most eight pasted URLs and 32 resulting files, 256 MiB per file, 512 MiB
per job; HTML parsing bounded to 1 MiB. Deduplicate exact URLs, follow at most five
redirects per resource, validate and pin every resolved HTTP destination. Reject
credentials, private/loopback/link-local/multicast/reserved destinations and proxy
inheritance. Only validated public addresses may be used in production. Tests use
an injected loopback-only connector, never arbitrary Internet traffic.

LD03: preflight every owned write, private directories/files, no remote-supplied
paths, bounded streaming buffers, deadlines and cancellation. Node adapters own
one download operation and cleanup on failure/cancel/close; WebView gets progress,
resource names/sizes, warnings and opaque selection IDs, never native paths.

LD04: pasted text and existing selected files survive errors/cancellation. Success
adds resources to the selected set and removes resolved URLs from the text; other
text remains. Clicking send during preparation cannot send an incomplete set.
The Stop control cancels download. No repeated automatic retries. Browser-only or
older native adapters report unsupported download explicitly.

CI must cover redirects into private space, malicious URLs/filenames, oversized
and chunked bodies, HTML resource selection/deduplication, timeout/cancel/cleanup,
selection composition and actual downloaded bytes through the Rust loopback engine.
Signed package/install/runtime evidence remains distinct from source/CI evidence.

LD05: offline.capabilities must preserve package-owned feature announcements through
the native tool registry and worker wire response. A backend-only feature check is
insufficient: CI must assert link-download-v1 on the actual worker response.
