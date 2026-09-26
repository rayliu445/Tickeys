#!/bin/bash
# Create (once) and verify a local self-signed code-signing identity.
#
# Why this exists: macOS Accessibility authorization is keyed to the app's
# code signature. With ad-hoc signatures (or none), every rebuild produces a
# different signature, so the system treats the new build as a *different*
# app and the accessibility grant is lost -- the infamous
# "keeps asking for permission" loop.
#
# Signing every build with the SAME local certificate gives the app a stable
# identity: the user grants Accessibility ONCE, and future rebuilds/upgrades
# keep working without touching System Settings again.
#
# Usage:  scripts/setup-signing.sh [identity-name]
# Default identity name: "Tickeys Local"

set -euo pipefail

IDENTITY="${1:-Tickeys Local}"

if security find-identity 2>/dev/null | grep -qF "\"$IDENTITY\""; then
    echo "✅ signing identity \"$IDENTITY\" already exists in your keychain"
    exit 0
fi

echo "Creating self-signed code-signing identity \"$IDENTITY\" ..."

WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

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
    -passout pass:tickeys-tmp \
    -certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES -macalg sha1

security import "$WORKDIR/identity.p12" \
    -k "$HOME/Library/Keychains/login.keychain-db" \
    -P tickeys-tmp \
    -T /usr/bin/codesign

# Optional: mark the certificate trusted for code signing. Signing itself
# works without this; only `security find-identity -p codesigning` treats the
# identity as "invalid" until then. If the confirmation dialog is in the way,
# this step can be skipped entirely.
if [ "${TRUST_CERT:-0}" = "1" ]; then
    security add-trusted-cert -p codeSign \
        -k "$HOME/Library/Keychains/login.keychain-db" \
        "$WORKDIR/cert.pem" || \
        echo "⚠️  skipped trust setup (signing still works); you can also set"
        echo "    Trust -> Code Signing = \"Always Trust\" manually in Keychain Access."
fi

if security find-identity 2>/dev/null | grep -qF "\"$IDENTITY\""; then
    echo "✅ created signing identity \"$IDENTITY\""
else
    echo "❌ identity creation failed" >&2
    exit 1
fi
