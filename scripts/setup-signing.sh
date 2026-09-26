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

# Mark the certificate trusted for code signing. Without this step the
# Accessibility permission silently never applies: tccd evaluates the app's
# designated requirement against trust settings, and an untrusted cert fails
# even though the user ticked the checkbox in System Settings.
# macOS may pop a confirmation dialog -- click OK / enter your password.
#
# IMPORTANT: trust the cert as it exists in the keychain (export it back by
# name), NOT the freshly generated file -- the two can differ if an identity
# with the same name already existed, and trusting the wrong one makes the
# grant never stick.
CERT_PEM="$WORKDIR/imported.pem"
if ! security find-certificate -c "$IDENTITY" -p \
    "$HOME/Library/Keychains/login.keychain-db" > "$CERT_PEM" 2>/dev/null; then
    CERT_PEM="$WORKDIR/cert.pem"
fi
if ! security add-trusted-cert -p codeSign \
    -k "$HOME/Library/Keychains/login.keychain-db" \
    "$CERT_PEM"; then
    echo "⚠️  automatic trust setup failed. The Accessibility permission will NOT"
    echo "    work until the cert is trusted: Keychain Access -> \"$IDENTITY\" ->"
    echo "    Trust -> Code Signing = \"Always Trust\", then re-grant the permission."
fi

if security find-identity -v -p codesigning 2>/dev/null | grep -qF "\"$IDENTITY\""; then
    echo "✅ created and trusted signing identity \"$IDENTITY\""
else
    echo "❌ identity creation failed or is not trusted" >&2
    exit 1
fi
