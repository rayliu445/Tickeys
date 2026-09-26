#!/bin/bash
# Create (once) the local code-signing setup for Tickeys.
#
# Why signing at all:
#   macOS ties the Accessibility authorization to the app's code signature.
#   Ad-hoc signatures change on every rebuild, so the system treats each new
#   build as a different app and the permission grant is lost -- the infamous
#   "keeps asking for permission" loop. Signing every build with the SAME
#   local certificate gives the app a stable identity: the user grants
#   Accessibility ONCE, and future rebuilds/upgrades keep working.
#
# Why a dedicated keychain:
#   A self-signed key imported into the login keychain triggers key-access
#   confirmation prompts (codesign can silently hang waiting for one). A
#   dedicated keychain with a known password lets us set the partition list
#   ourselves -- no prompts, fully scripted.
#
# Usage: scripts/setup-signing.sh          (idempotent; safe to re-run)

set -euo pipefail

IDENTITY="Tickeys Local"
KC_NAME="tickeys-signing.keychain-db"
KC="$HOME/Library/Keychains/$KC_NAME"
KC_PASS="tickeys-local"

HASH="$(security find-identity -v -p codesigning 2>/dev/null \
        | grep -F "\"$IDENTITY\"" | head -1 | awk '{print $2}' || true)"
if [ -n "$HASH" ]; then
    echo "✅ signing identity already set up (certificate $HASH)"
    exit 0
fi

echo "==> creating signing keychain + certificate (fully automatic)"
WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

security create-keychain -p "$KC_PASS" "$KC"
security unlock-keychain -p "$KC_PASS" "$KC"
security list-keychains -s "$KC" $(security list-keychains | tr -d '"') >/dev/null

openssl req -newkey rsa:2048 -nodes \
    -keyout "$WORKDIR/key.pem" \
    -x509 -days 3650 \
    -out "$WORKDIR/cert.pem" \
    -subj "/CN=$IDENTITY" \
    -addext "keyUsage=digitalSignature" \
    -addext "extendedKeyUsage=codeSigning"

openssl pkcs12 -export \
    -out "$WORKDIR/identity.p12" \
    -inkey "$WORKDIR/key.pem" \
    -in "$WORKDIR/cert.pem" \
    -passout pass:import-tmp \
    -certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES -macalg sha1

security import "$WORKDIR/identity.p12" -k "$KC" -P import-tmp -T /usr/bin/codesign

# Whitelist codesign on this keychain's keys -- no confirmation prompts ever.
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KC_PASS" "$KC" >/dev/null

# Trust the certificate for code signing, so the Accessibility grant applies.
security find-certificate -c "$IDENTITY" -p "$KC" > "$WORKDIR/trust.pem"
security add-trusted-cert -p codeSign \
    -k "$HOME/Library/Keychains/login.keychain-db" \
    "$WORKDIR/trust.pem"

HASH="$(security find-identity -v -p codesigning 2>/dev/null \
        | grep -F "\"$IDENTITY\"" | head -1 | awk '{print $2}')"
if [ -z "$HASH" ]; then
    echo "❌ identity creation failed" >&2
    exit 1
fi

echo "✅ signing identity ready (certificate $HASH)"
echo "   next: scripts/build.sh [--install]"
