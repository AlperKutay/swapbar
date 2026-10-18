#!/usr/bin/env bash
# Package target/Swapbar.app into target/Swapbar.dmg (drag-to-Applications layout).
# Run scripts/bundle.sh first.
#
# Optional, for a signed and notarized DMG:
#   SIGN_IDENTITY="Developer ID Application: …"
#   NOTARY_PROFILE=<keychain profile from `xcrun notarytool store-credentials`>
set -euo pipefail
cd "$(dirname "$0")/.."

APP="target/Swapbar.app"
DMG="target/Swapbar.dmg"
[[ -x "$APP/Contents/MacOS/swapbar" ]] || { echo "missing $APP, run scripts/bundle.sh first" >&2; exit 1; }

# Window layout (background, icon positions) lives in assets/dmg/settings.py; dmgbuild writes the
# .DS_Store directly, so this works headless on CI without driving Finder.
DMGBUILD_VERSION=1.6.7
if command -v uvx >/dev/null; then
  DMGBUILD=(uvx --quiet "dmgbuild==$DMGBUILD_VERSION")
else
  VENV="target/.dmgbuild-venv"
  [[ -x "$VENV/bin/dmgbuild" ]] || { python3 -m venv "$VENV" && "$VENV/bin/pip" install --quiet "dmgbuild==$DMGBUILD_VERSION"; }
  DMGBUILD=("$VENV/bin/dmgbuild")
fi

rm -f "$DMG"
"${DMGBUILD[@]}" -s assets/dmg/settings.py -D app="$PWD/$APP" -D assets="$PWD/assets" Swapbar "$DMG" 2>&1 >/dev/null | { grep -v deprecated || true; } >&2

if [[ -n "${SIGN_IDENTITY:-}" ]]; then
  codesign --force --timestamp --sign "$SIGN_IDENTITY" "$DMG"
  if [[ -n "${NOTARY_PROFILE:-}" ]]; then
    xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" --wait
    # Apple's ticket service drops connections now and then; stapling is safe to retry.
    for attempt in 1 2 3 4 5; do
      xcrun stapler staple "$DMG" && break
      [[ $attempt == 5 ]] && exit 1
      echo "stapler staple failed (attempt $attempt), retrying in $((attempt * 10))s" >&2
      sleep $((attempt * 10))
    done
  fi
fi

shasum -a 256 "$DMG" | sed 's|target/||' > "$DMG.sha256"
echo "built: $DMG ($(du -h "$DMG" | cut -f1))"
