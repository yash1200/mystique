#!/usr/bin/env bash
# Signed (and, with credentials, notarized) macOS release build.
#
# Tauri reads these from the environment and they override tauri.conf.json,
# where signingIdentity stays "-" (ad-hoc) so plain local builds still work.
#
#   APPLE_SIGNING_IDENTITY  "Developer ID Application: Name (TEAMID)"
#                           or a local identity from scripts/dev-signing-identity.sh
# Notarization, either:
#   APPLE_ID, APPLE_PASSWORD (app-specific password), APPLE_TEAM_ID
# or an App Store Connect API key:
#   APPLE_API_ISSUER, APPLE_API_KEY, APPLE_API_KEY_PATH
#
# Usage: scripts/release-macos.sh [extra `tauri build` args, e.g. --target universal-apple-darwin]
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
	echo "APPLE_SIGNING_IDENTITY is not set. Without it the build is ad-hoc signed and" >&2
	echo "the keychain asks again after every update. See docs/RELEASING.md." >&2
	exit 1
fi

if ! security find-identity -v -p codesigning | grep -Fq "$APPLE_SIGNING_IDENTITY"; then
	echo "No valid code-signing identity matching: $APPLE_SIGNING_IDENTITY" >&2
	security find-identity -v -p codesigning >&2 || true
	exit 1
fi

notarize=no
if [[ -n "${APPLE_ID:-}" && -n "${APPLE_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
	notarize=yes
elif [[ -n "${APPLE_API_ISSUER:-}" && -n "${APPLE_API_KEY:-}" && -n "${APPLE_API_KEY_PATH:-}" ]]; then
	notarize=yes
fi
if [[ "$APPLE_SIGNING_IDENTITY" == Developer\ ID\ Application* && "$notarize" == no ]]; then
	echo "Warning: Developer ID build without notarization credentials." >&2
	echo "Gatekeeper will block it on other Macs." >&2
fi

pnpm tauri build "$@"

# Workspace builds land in ./target (or <triple>/release with --target).
app=$(find target -maxdepth 5 -type d -path '*/release/bundle/macos/*.app' -print0 2>/dev/null \
	| xargs -0 ls -dt 2>/dev/null | head -n 1)
if [[ -z "$app" ]]; then
	echo "Built .app not found under target/" >&2
	exit 1
fi

echo "Verifying $app"
codesign --verify --deep --strict --verbose=2 "$app"
codesign --display --requirements - "$app" 2>&1 | sed -n 's/^designated => /designated requirement: /p'
if codesign --display --verbose=2 "$app" 2>&1 | grep -q 'flags=.*runtime'; then
	echo "hardened runtime: on"
else
	echo "hardened runtime: OFF" >&2
	exit 1
fi
if [[ "$notarize" == yes ]]; then
	xcrun stapler validate "$app"
	spctl --assess --type execute --verbose=2 "$app"
fi
echo "Done: $app"
