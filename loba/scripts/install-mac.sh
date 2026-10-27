#!/usr/bin/env bash
# Install the DMG that build-mac.sh just made, replacing the copy in
# /Applications, then open it.
#
#   ./scripts/install-mac.sh            install ~/Downloads/L.O.B.A_*.dmg (newest)
#   ./scripts/install-mac.sh --reset    also clear L.O.B.A's Accessibility and
#                                       Input Monitoring entries (only needed if
#                                       the permission screen stays on "Waiting")
set -euo pipefail

say() { printf '\033[1m%s\033[0m\n' "$*"; }
fail() { printf '\033[31m%s\033[0m\n' "$*" >&2; exit 1; }

APP_ID="app.loba.desktop"
DEST="/Applications/L.O.B.A.app"

dmg=$(ls -t "$HOME"/Downloads/L.O.B.A_*.dmg 2>/dev/null | head -1 || true)
[[ -n "$dmg" ]] || fail "No L.O.B.A_*.dmg in ~/Downloads. Run ./scripts/build-mac.sh first."
say "Installing $(basename "$dmg") ($(date -r "$dmg" '+%d %b %H:%M'))"

osascript -e 'quit app "L.O.B.A"' >/dev/null 2>&1 || true
sleep 1

# An older DMG left mounted has the same volume name; detach them all so the
# copy below can only come from this one.
for v in /Volumes/L.O.B.A*; do
  [[ -d "$v" ]] && hdiutil detach "$v" -quiet -force || true
done
mnt="$(mktemp -d)"
hdiutil attach "$dmg" -nobrowse -quiet -mountpoint "$mnt" || fail "Couldn't open the DMG."
[[ -d "$mnt/L.O.B.A.app" ]] || { hdiutil detach "$mnt" -quiet; fail "The DMG has no L.O.B.A.app."; }

rm -rf "$DEST"
cp -R "$mnt/L.O.B.A.app" "$DEST"
hdiutil detach "$mnt" -quiet || true
rmdir "$mnt" 2>/dev/null || true
xattr -dr com.apple.quarantine "$DEST" 2>/dev/null || true

if [[ "${1:-}" == "--reset" ]]; then
  tccutil reset Accessibility "$APP_ID" >/dev/null 2>&1 || true
  tccutil reset ListenEvent "$APP_ID" >/dev/null 2>&1 || true
  say "Cleared permissions; grant Accessibility again when asked."
fi

signer=$(codesign -dvv "$DEST" 2>&1 | sed -n 's/^Authority=//p' | head -1)
say "Signed by: ${signer:-ad-hoc}"
open "$DEST"
