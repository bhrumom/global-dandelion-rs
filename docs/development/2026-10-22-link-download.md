# Link download implementation

New dandelion-download Rust adapter performs bounded streaming of public URLs and
one level of HTML resources. Native job ownership, staging, opaque selection IDs,
progress and cancellation are provided by the installed package, with no Host
business code changes. The composer starts preparation on paste and sends actual
staged bytes through the existing engine when requested.

Actions-only verification covers controlled loopback HTTP via a test-only resolver,
private redirects, limits, HTML extraction, native selection composition, loopback
payload receipt and browser paste/download/cancel behavior. No arbitrary public
hosts are used as CI fixtures. Login/script-generated/streaming-only resources are
not promised as downloadable files. Source/CI does not establish package signing,
installation, global-node readiness or mobile device runtime.
