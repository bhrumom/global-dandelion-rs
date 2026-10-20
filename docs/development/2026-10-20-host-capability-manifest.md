# Host capability manifest correction

The 0.3.0 mini-app package manifest keeps `requestedCapabilities` empty. Native
tools are not package self-grants: Fabushi Host discovers and authorizes the
current platform's implementation at runtime through
`fabushi.miniapp.tools.v1`, and rechecks that contract for each invocation.

This is required by the catalog package policy and the production signer. It
does not remove the transfer, scan, state, config, platform, hotspot, update,
diagnostics or compatibility entry points from the UI or Host bridge. Those
panels remain capability-gated and are exposed only when the installed Host
actually advertises the corresponding real implementation.

Source baseline: `main` at the preceding capability-contract revision
`16e5eeb0ea939b62e228c03fa6c06762435781af`. A new exact-head CI preparation
artifact and production signing result are required before delivery claims are
updated.
