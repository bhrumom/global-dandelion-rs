# ADR 0005: independently trusted signed manifest and staged updates

Status: implemented pending exact-revision verification. Spec v2 C21/C25.

Operator-supplied raw Ed25519 public key (32 bytes) validates a detached signature
(64 bytes) over the exact manifest bytes before JSON parsing. Schema, semantic
version, stable channel, explicit rollback, target, expiry and bounded artifact
size are validated. An independent trusted key is required; a downloaded hash
alone never establishes trust. The artifact must match signed SHA-256 and size.

Sources can be local files or explicit HTTPS metadata/artifact origins. HTTPS-only,
no URL credentials/redirects/system proxy, request timeouts and input bounds are
mandatory. Artifacts require an explicit allowed hostname. Stage into a private
versioned temporary bundle, sync files and directory, lock promotion and rename;
never overwrite an existing bundle or execute/replace the running binary.
Activation remains an explicit service-manager/operator operation. An automatic
polling/activation service and production signing-key provisioning remain separate
acceptance work, not implied by check/stage commands. Synthetic CI keys are never
trusted in production. Tests cover native CLI verification, tampered metadata,
wrong artifacts, atomic staging, exact copies and no-overwrite behavior.
