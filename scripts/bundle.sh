#!/usr/bin/env bash
# Build a release binary and wrap it in a minimal .app bundle (LSUIElement=1 → no Dock icon).
#
#   ./scripts/bundle.sh              # host architecture only
#   ./scripts/bundle.sh --universal  # arm64 + x86_64 (needs: rustup target add x86_64-apple-darwin)
#
# SIGN_IDENTITY="Developer ID Application: …" signs with hardened runtime (needed for notarization).
# Without it the bundle is ad-hoc signed.
set -euo pipefail
cd "$(dirname "$0")/.."

UNIVERSAL=0
[[ "${1:-}" == "--universal" ]] && UNIVERSAL=1

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
export SWAPBAR_BUILT="Built $(date "+%-d %b %Y %H:%M")"

APP="target/Swapbar.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

if [[ $UNIVERSAL == 1 ]]; then
  # A Homebrew rust on PATH has no x86_64 std; prefer rustup's toolchain when present.
  if command -v rustup >/dev/null; then
    PATH="$(dirname "$(rustup which cargo)"):$PATH"
  fi
  cargo build --release --target aarch64-apple-darwin
  cargo build --release --target x86_64-apple-darwin
  lipo -create -output "$APP/Contents/MacOS/swapbar" \
    target/aarch64-apple-darwin/release/swapbar \
    target/x86_64-apple-darwin/release/swapbar
else
  cargo build --release
  cp target/release/swapbar "$APP/Contents/MacOS/swapbar"
fi

# Cargo's `strip = true` is not applied by every toolchain (rustup's 1.99 left ~1 MB of symbols),
# so strip explicitly. -x keeps the global symbols the dynamic linker needs.
strip -x "$APP/Contents/MacOS/swapbar"

cp assets/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Swapbar</string>
  <key>CFBundleDisplayName</key><string>Swapbar</string>
  <key>CFBundleIdentifier</key><string>com.miova.swapbar</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleExecutable</key><string>swapbar</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSUIElement</key><true/>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>© 2026 Mustafa Afat. MIT License.</string>
</dict>
</plist>
PLIST

if [[ -n "${SIGN_IDENTITY:-}" ]]; then
  codesign --force --options runtime --timestamp --sign "$SIGN_IDENTITY" "$APP"
else
  codesign --force --sign - "$APP"
fi

echo "built: $APP ($VERSION, $(lipo -archs "$APP/Contents/MacOS/swapbar"))"
