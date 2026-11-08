#!/usr/bin/env bash
# Optional: a stable, self-signed code-signing identity for local release builds.
#
# Ad-hoc builds ("-") get a new code hash on every build, so the keychain
# forgets "Always Allow" each time. Signing every local build with the same
# certificate keeps the app's identity stable, so allowing the vault key once
# is enough. Debug builds (`pnpm tauri dev`) don't need this: they keep the
# vault key in a 0600 file and never touch the keychain.
#
# Not for distribution: other Macs won't trust this certificate.
#
# Run once:  scripts/dev-signing-identity.sh
# Then:      APPLE_SIGNING_IDENTITY="Mystique Local Signing" scripts/release-macos.sh
#
# macOS asks for your login password when the certificate is trusted for
# code signing. Remove it later in Keychain Access (search the name).
set -euo pipefail

NAME="${1:-Mystique Local Signing}"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

if security find-identity -v -p codesigning | grep -Fq "\"$NAME\""; then
	echo "Identity \"$NAME\" already exists."
	exit 0
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

cat >"$work/cert.conf" <<EOF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
EOF

openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
	-keyout "$work/key.pem" -out "$work/cert.pem" -config "$work/cert.conf" 2>/dev/null
pass=$(openssl rand -hex 16)
openssl pkcs12 -export -legacy -inkey "$work/key.pem" -in "$work/cert.pem" \
	-name "$NAME" -out "$work/identity.p12" -passout "pass:$pass" 2>/dev/null \
	|| openssl pkcs12 -export -inkey "$work/key.pem" -in "$work/cert.pem" \
		-name "$NAME" -out "$work/identity.p12" -passout "pass:$pass"

security import "$work/identity.p12" -k "$KEYCHAIN" -P "$pass" -T /usr/bin/codesign
security add-trusted-cert -r trustRoot -p codeSign -k "$KEYCHAIN" "$work/cert.pem"

security find-identity -v -p codesigning | grep -F "\"$NAME\""
echo "Created \"$NAME\". Build with:"
echo "  APPLE_SIGNING_IDENTITY=\"$NAME\" scripts/release-macos.sh"
