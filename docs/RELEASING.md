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
the other. Per-connection keychain items from builds before the vault are not
read any more; Settings › Passwords can delete them.

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

## Publishing a release (auto-update)

Installed apps check
`https://github.com/yash1200/mystique/releases/latest/download/latest.json`
at launch (and from **Mystique › Check for Updates…** or **Settings › About**).
If it names a newer version, a card in the bottom-right offers it. **Update**
downloads in the background, installs, then asks to restart.

Two steps:

```bash
pnpm release 1.0.9                   # notes drafted from the commits since the last tag
pnpm release 1.0.9 --notes notes.md  # …or your own Markdown notes
pnpm release 1.0.9 --dry-run         # checks and notes only
```

then **Actions › release › Run workflow** with `v1.0.9` (or `1.0.9`).

`pnpm release` (`scripts/release.mjs`) checks everything first: on `main`, a
clean tree equal to `origin/main`, a newer version, no existing tag or
release, GitHub push access. Then it bumps the version in `package.json`,
`src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` and `Cargo.lock`,
commits and pushes, tags `v<version>` and pushes the tag, and publishes the
GitHub release, not yet marked latest. If it stops part way, run the same
command again: a bump or tag already pushed is reused.

The workflow builds every platform into that release, writes `latest.json`,
and only then marks the release latest, so installed apps never see a newest
release without update files.

`gh` must be able to push to the repo: the script uses `GH_TOKEN`, else
`gh auth token -u yash1200`.

### The updater key

Updates are signed with a minisign key, separate from Apple signing. The
public half is `plugins.updater.pubkey` in `tauri.conf.json`; the private
half is `~/.tauri/mystique.key` (password protected).

```bash
# Once: keep the key password in the login keychain (for local signed builds).
security add-generic-password -a "$USER" -s mystique-updater-key -w
```

Or set `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. **Back up the key file and its
password.** Installed apps only accept updates signed with this key; if it is
lost, every user has to download the next version by hand.

A plain `pnpm tauri build` needs no key: only the release config asks for
the update archive.

Builds from before 1.0.3 have no updater, so those users install the first
version with it by hand.

## CI

`.github/workflows/release.yml` is run by hand for an existing tag
(Actions › release › Run workflow). It builds macOS (Apple silicon, Intel),
Windows (x64, ARM64 NSIS installers) and Linux (x64, ARM64: `.deb`, `.rpm`,
`.AppImage`), all ad-hoc / unsigned. The files go to the tag's release: the
one `pnpm release` published, or a draft if there is none (same names are
replaced). `latest.json` is then rebuilt from every update signature, keeping
platforms already in it, and the release is published and marked latest. A
tag push does not start it, because `pnpm release` pushes the tag itself.
Linux builds run on Ubuntu 26.04, so they need a distro at least that new.
Repository secrets: the updater
key as `TAURI_SIGNING_PRIVATE_KEY` (the key file's text) and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

For a Developer ID build later, pass `APPLE_CERTIFICATE` (base64 `.p12`),
`APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`,
`APPLE_PASSWORD` and `APPLE_TEAM_ID` to the build step on macOS.

```bash
base64 -i DeveloperID.p12 | pbcopy   # paste as APPLE_CERTIFICATE
```
