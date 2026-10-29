#!/usr/bin/env bash
# Install the app GitHub built, without building on this Mac (no 6 GB needed).
#
#   1. On GitHub: Actions → the latest "L.O.B.A — macOS" run → Artifacts →
#      L.O.B.A-macos-arm64-… (downloads a zip into ~/Downloads).
#   2. ./scripts/install-from-github.sh
#
# Takes the newest download (zip, the folder Safari unzips it into, or the
# DMG), re-signs the app with this Mac's "L.O.B.A Local Signing" certificate
# so Accessibility and the microphone stay granted, then installs it the same
# way install-mac.sh does.
set -euo pipefail

say() { printf '\033[1m%s\033[0m\n' "$*"; }
fail() { printf '\033[31m%s\033[0m\n' "$*" >&2; exit 1; }

cd "$(dirname "$0")/.."
DEST="/Applications/L.O.B.A.app"
SIGN_NAME="L.O.B.A Local Signing"
work="$(mktemp -d)"
trap 'hdiutil detach "$work/mnt" -quiet 2>/dev/null || true; rm -rf "$work"' EXIT

# --- find the download --------------------------------------------------------
got=$(ls -td "$HOME"/Downloads/L.O.B.A-macos-arm64-* 2>/dev/null | head -1 || true)
[[ -n "$got" ]] || fail "No L.O.B.A-macos-arm64-… download in ~/Downloads. Download it from GitHub first (see the top of this script)."
say "Using $(basename "$got") ($(date -r "$got" '+%d %b %H:%M'))"
if [[ -f "$got" && "$got" == *.zip ]]; then
  ditto -x -k "$got" "$work/unzipped" || fail "Couldn't unzip $(basename "$got")."
  got="$work/unzipped"
fi
dmg=$(find "$got" -maxdepth 2 -name '*.dmg' | head -1)
[[ -n "$dmg" ]] || fail "There's no .dmg inside $(basename "$got")."

mkdir "$work/mnt"
hdiutil attach "$dmg" -nobrowse -quiet -mountpoint "$work/mnt" || fail "Couldn't open the DMG."
[[ -d "$work/mnt/L.O.B.A.app" ]] || fail "The DMG has no L.O.B.A.app."
app="$work/L.O.B.A.app"
ditto "$work/mnt/L.O.B.A.app" "$app"
hdiutil detach "$work/mnt" -quiet || true
xattr -dr com.apple.quarantine "$app" 2>/dev/null || true

# --- sign as this Mac, like build-mac.sh -----------------------------------------
ent="$work/entitlements.plist"
cp src-tauri/Entitlements.plist "$ent"
/usr/libexec/PlistBuddy -c "Add :com.apple.security.cs.disable-library-validation bool true" "$ent" >/dev/null 2>&1 || true
if security find-certificate -c "$SIGN_NAME" >/dev/null 2>&1 &&
  codesign --force --deep --options runtime --entitlements "$ent" --sign "$SIGN_NAME" "$app"; then
  say "Signed with \"$SIGN_NAME\"."
else
  printf '\033[33mNo local certificate (or it refused); signing ad-hoc. Accessibility may need granting again.\033[0m\n'
  codesign --force --deep --options runtime --entitlements src-tauri/Entitlements.plist --sign - "$app"
fi
codesign --verify --deep --strict "$app" || fail "The signed app didn't verify."

# --- install -----------------------------------------------------------------------
osascript -e 'quit app "L.O.B.A"' >/dev/null 2>&1 || true
for _ in 1 2 3 4 5; do
  pgrep -f "$DEST/Contents/MacOS/" >/dev/null || break
  sleep 1
done
pkill -f "$DEST/Contents/MacOS/" 2>/dev/null && sleep 1 || true

rm -rf "$DEST"
ditto "$app" "$DEST"
say "Installed from GitHub's build. Opening L.O.B.A."
open "$DEST"
