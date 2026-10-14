# ADR 0006: explicit NetworkManager capability with owned rollback

Status: implemented; real wireless hardware acceptance outstanding. Spec C20.

The core ProcessRunner port cannot execute processes itself. Linux whitelists
/usr/bin/nmcli, argv only, fixed environment, null stdin, bounded stdout/stderr,
20-second operation timeouts and child termination. No shell, hidden sudo or
interactive password prompt. Existing NetworkManager/Polkit authorization is
required; unavailable capabilities fail rather than changing privileges.

Plan is pure/read-only. Apply/stop require explicit --confirm and a private
ownership journal. An AP-capability check precedes changes. Apply creates a UUID-
scoped temporary save=no WPA-PSK AP profile with autoconnect disabled. Only kernel/
NetworkManager-regulated a/bg bands are requested; no country/DFS/channel forcing.
Password material is read from a permission-checked file and supplied through a
short-lived private nmcli passwd-file, never command arguments or log output.

A synced journal records prior connection identity before any profile creation.
Rollback verifies the owned profile id/UUID, only deletes that profile, and restores
the previous connection only when doing so does not override a later operator
change. Failed rollback is recorded as rollback-required; explicit stop retries.
On abrupt process termination the journal is retained for recovery. The password
file is ephemeral but an unclean host crash can leave a private temporary file;
operators should protect the journal directory and audit recovery on real hardware.

Mock capability tests cover wiring/ownership/rollback and CLI tests cover plan and
consent. These are NOT a claim of hardware AP activation or driver compatibility.
The actual user's network is never changed during development/CI.
