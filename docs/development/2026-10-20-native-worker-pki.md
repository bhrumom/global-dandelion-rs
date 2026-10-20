# Native worker PKI fixture provider

The new two-worker mTLS gate failed on 0f18abc/14896b5/27e0f8f before
receiver readiness. The adapter reported a stable exit code, and the fixture's
independent invocation of the same native receiver also rejected the generated
credentials. No authentication check was relaxed.

Discovery: existing native-Apple acceptance explicitly selects Homebrew OpenSSL 3
through GITHUB_PATH. The new package workflow had omitted that provider pin and
used macOS's different system openssl. Reuse the existing provider pin and assert
its major version before constructing ephemeral PKI. Continue to require actual
mutual authentication, readiness, verified payload and cancellation. The next
exact-head Actions result is required before claiming the correction passed.
