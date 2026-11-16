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

Releases are Apple silicon only for now. Installed apps check
`https://github.com/yash1200/mystique/releases/latest/download/latest.json`
at launch (and from **Mystique › Check for Updates…** or **Settings › About**).
If it names a newer version, a card in the bottom-right offers it. **Update**
downloads in the background, installs, then asks to restart.

One command does it all (`scripts/ship.sh`):

```bash
pnpm ship 1.0.4                    # notes drafted from the commits since the last tag
pnpm ship 1.0.4 --edit             # …opened in $EDITOR first
pnpm ship 1.0.4 --notes notes.md   # your own Markdown notes
pnpm ship 1.0.4 --dry-run          # checks and notes only
```

It checks everything first (clean `main` equal to `origin/main`, a newer
version, no existing tag or release, GitHub push access, the updater key),
then bumps, commits and pushes the version, and runs `pnpm release`. If the
build or upload fails after the push, run the same command again: it skips
the bump and carries on. By hand, the same steps are:

```bash
pnpm bump 1.0.4
git commit -am "chore(release): version 1.0.4" && git push
pnpm release --notes notes.md      # Markdown shown on the release page
```

`pnpm release` (`scripts/release.mjs`) refuses to run unless the tree is clean
and `HEAD` is `origin/main`. It builds with `src-tauri/tauri.release.conf.json`
(which turns on the signed update archive), writes `latest.json`, tags
`v<version>`, and publishes the release with the `.dmg`, the
`.app.tar.gz`, its `.sig` and `latest.json`. `--dry-run` stops before tagging;
`--skip-build` reuses the last build. Set `APPLE_SIGNING_IDENTITY` (and the
notarization variables above) to ship a Developer ID build.

`gh` must be able to push to the repo: `gh auth switch -u yash1200`, or
`GH_TOKEN=$(gh auth token -u yash1200) pnpm release …`.

### The updater key

Updates are signed with a minisign key, separate from Apple signing. The
public half is `plugins.updater.pubkey` in `tauri.conf.json`; the private
half is `~/.tauri/mystique.key` (password protected).

```bash
# Once: keep the key password in the login keychain for `pnpm release`.
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

`.github/workflows/release-macos.yml` is run by hand for an existing tag
(Actions › release-macos › Run workflow). It builds Apple silicon and
attaches the files, `latest.json` included, to a draft release. A tag push
does not start it, because `pnpm release` pushes the tag itself. Repository
secrets: `APPLE_CERTIFICATE` (base64 `.p12`), `APPLE_CERTIFICATE_PASSWORD`,
`APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`, and
the updater key as `TAURI_SIGNING_PRIVATE_KEY` (the key file's text) and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

```bash
base64 -i DeveloperID.p12 | pbcopy   # paste as APPLE_CERTIFICATE
```
