# Releasing (macOS)

## Why signing matters here

macOS ties keychain access ("Always Allow") to the app's code signature.
`tauri.conf.json` keeps `signingIdentity: "-"` (ad-hoc) so plain local builds
work without certificates. But an ad-hoc signature changes on every build, so
the keychain asks again after each rebuild or update.

Mystique keeps all saved secrets in one encrypted vault behind a single
keychain item, so a release build asks at most once. With a stable signature
it stops asking for good.

| Build | Vault key | Keychain prompts |
|---|---|---|
| `pnpm tauri dev` (debug) | keychain key, cached in a 0600 `vault.key` in the app data dir | at most once per machine (first read), then none |
| Local release, ad-hoc | keychain | once after each rebuild |
| Local release, `scripts/dev-signing-identity.sh` | keychain | once, ever |
| Developer ID + notarized | keychain | once, ever (survives updates) |

Dev and release builds share the same key, so secrets saved in one open in
the other. Old per-connection keychain items move into the vault the first
time each connection is used (one read each, once).

`MYSTIQUE_VAULT_KEY_SOURCE=keychain` forces keychain-only in any build;
`MYSTIQUE_VAULT_KEY_SOURCE=file` uses only the file and never touches the
keychain (CI, headless).

## Signed release (Developer ID)

Needs an Apple Developer account and a "Developer ID Application" certificate
in your login keychain.

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)"
# Notarization: app-specific password from appleid.apple.com
export APPLE_ID="you@example.com" APPLE_PASSWORD="xxxx-xxxx-xxxx-xxxx" APPLE_TEAM_ID="TEAMID"
scripts/release-macos.sh                      # host architecture
scripts/release-macos.sh --target universal-apple-darwin
```

Tauri reads these variables, signs with hardened runtime, notarizes, and
staples. The script then checks the signature, the hardened-runtime flag,
the staple and Gatekeeper. An App Store Connect API key works too
(`APPLE_API_ISSUER`, `APPLE_API_KEY`, `APPLE_API_KEY_PATH`).

## Local builds without an Apple account

```bash
scripts/dev-signing-identity.sh                                   # once
APPLE_SIGNING_IDENTITY="Mystique Local Signing" scripts/release-macos.sh
```

This creates a self-signed code-signing certificate in your login keychain
(macOS asks for your password to trust it). Builds signed with it keep the
same identity, so "Always Allow" sticks. Other Macs won't trust it.

## CI

`.github/workflows/release-macos.yml` builds both architectures on a `v*`
tag and attaches them to a draft GitHub release. Repository secrets:
`APPLE_CERTIFICATE` (base64 `.p12`), `APPLE_CERTIFICATE_PASSWORD`,
`APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`.

```bash
base64 -i DeveloperID.p12 | pbcopy   # paste as APPLE_CERTIFICATE
```
