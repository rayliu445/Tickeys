#!/bin/bash
# Build Tickeys.app: compile (native arch), assemble the bundle, sign it.
#
# Usage:
#   scripts/build.sh              # build into build/Tickeys.app
#   scripts/build.sh --install    # also install to /Applications
#
# Prerequisites:
#   - Rust (rustup or Homebrew), any recent version
#   - scripts/setup-signing.sh run once (creates the "Tickeys Local" identity)
#
# The binary is built for the machine's native architecture. Intel Macs get an
# Intel build, Apple Silicon gets an arm64 build -- no Rosetta involved.

set -euo pipefail
cd "$(dirname "$0")/.."

# Resolve the identity HASH (not the name) -- avoids ambiguity when several
# keychains contain a certificate with the same common name.
IDENTITY="${SIGN_IDENTITY:-$(security find-identity -v -p codesigning 2>/dev/null \
    | grep -F '"Tickeys Local"' | head -1 | awk '{print $2}')}"
if [ -z "$IDENTITY" ]; then
    echo "❌ no signing identity found. Run scripts/setup-signing.sh first." >&2
    exit 1
fi
INSTALL=0
[ "${1:-}" = "--install" ] && INSTALL=1

echo "==> building (native arch: $(uname -m))"
cargo build --release

echo "==> assembling app bundle"
APP="build/Tickeys.app"
rm -rf build
mkdir -p build
cp -R Tickeys.app "$APP"
mkdir -p "$APP/Contents/MacOS"
cp target/release/Tickeys "$APP/Contents/MacOS/Tickeys"

VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
plutil -replace CFBundleShortVersionString -string "$VERSION" "$APP/Contents/Info.plist"
plutil -replace CFBundleVersion -string "$VERSION" "$APP/Contents/Info.plist"

# 解锁专用签名钥匙串，避免 codesign 弹密码框
SIGN_KC="$HOME/Library/Keychains/tickeys-signing.keychain-db"
if [ -f "$SIGN_KC" ]; then
    security unlock-keychain -p tickeys-local "$SIGN_KC" 2>/dev/null || true
fi

echo "==> code signing with certificate $IDENTITY"
codesign --force --sign "$IDENTITY" "$APP"
codesign --verify --verbose=1 "$APP"

echo
echo "✅ built $APP (version $VERSION)"

if [ "$INSTALL" -eq 1 ]; then
    echo "==> installing to /Applications"
    # kill a possibly running instance so the files can be replaced
    osascript -e 'tell application "Tickeys" to quit' >/dev/null 2>&1 || true
    pkill -x Tickeys >/dev/null 2>&1 || true
    rm -rf /Applications/Tickeys.app
    cp -R "$APP" /Applications/Tickeys.app
    echo "✅ installed -- launch it from Applications"
fi
