# Package-owned public link downloads

Spec: link-download.md LD01..04. Use Rust reqwest with strict TLS, public-destination
validation, pinned resolved addresses and manual per-hop redirects. Parse bounded
HTML with Servo's scraper, fetch one level of explicit resources, never execute
page scripts. Stream through a private filesystem Sink with disk preflight and
owned subprocess cancellation. No Host download business code and no renderer fetch.

Extend the existing selection.compose contract with an explicit downloadLinks
option, returning an operation handle. Existing status and selection.release own
progress/cancellation. Only a fully staged selection enters the normal Rust send
path; partial-resource warnings remain visible before send. Prior selections and
input survive failures. Older adapters must explicitly announce link-download-v1
before UI uses the option. Mobile engine availability and actual installed-version
acceptance remain separate from source and browser/native CI tests.
