#!/usr/bin/env bash
# Build L.O.B.A.dmg on an Apple Silicon Mac, lean on disk.
#
#   ./scripts/build-mac.sh            build, put the DMG in ~/Downloads, delete the build tree
#   ./scripts/build-mac.sh --keep     same, but keep src-tauri/target for faster rebuilds
#   ./scripts/build-mac.sh --release  sign with your Developer ID and have Apple notarize it,
#                                     for giving to other people (setup: LOBA.md > Release)
#
# Why it's lean:
# - native build (no --target): one build tree, shared deps compiled once
# - no LTO for local builds: rlibs don't carry a second copy as LLVM bitcode,
#   and the final link takes minutes instead of tens of minutes
# - checks free space first and tells you exactly what's safe to clear
# - removes the build tree afterwards (the DMG is all you need)
set -euo pipefail

NEED_GB=6
KEEP=0
RELEASE=0
for arg in "$@"; do
  case "$arg" in
    --keep) KEEP=1 ;;
    --release) RELEASE=1 ;;
    *) printf 'Unknown option: %s\n' "$arg" >&2; exit 2 ;;
  esac
done
NOTARY_PROFILE="${LOBA_NOTARY_PROFILE:-LOBA_NOTARY}"

cd "$(dirname "$0")/.."
ROOT="$PWD"
TARGET="$ROOT/src-tauri/target"

say() { printf '\033[1m%s\033[0m\n' "$*"; }
fail() { printf '\033[31m%s\033[0m\n' "$*" >&2; exit 1; }

# --- toolchain -------------------------------------------------------------
# Conda (the "(base)" prompt) puts its own cmake/clang/python first on PATH,
# which breaks native builds. Drop it for this script only.
PATH="$(printf '%s' "$PATH" | tr ':' '\n' | grep -viE 'conda|miniforge|mambaforge' | paste -sd: -)"
export PATH="$HOME/.cargo/bin:$HOME/.bun/bin:/opt/homebrew/bin:$PATH"

[[ "$(uname -s)" == "Darwin" ]] || fail "This script builds the macOS app; run it on a Mac."
[[ "$(uname -m)" == "arm64" ]] || fail "Expected an Apple Silicon Mac (arm64)."
for tool in cargo bun cmake xcrun; do
  command -v "$tool" >/dev/null || fail "Missing '$tool'. See LOBA.md → Build for the one-time setup."
done
if [[ "$(command -v cargo)" == /opt/homebrew/* ]]; then
  fail "cargo resolves to Homebrew's Rust. Run 'brew uninstall rust' (rustup's is in ~/.cargo/bin)."
fi

# --- space -----------------------------------------------------------------
# A previous --target build left a second tree; it can't be reused here.
if [[ -d "$TARGET/aarch64-apple-darwin" ]]; then
  say "Removing the old cross-target build tree…"
  rm -rf "$TARGET/aarch64-apple-darwin"
fi

free_gb=$(( $(df -Pk "$ROOT" | awk 'NR==2 {print $4}') / 1024 / 1024 ))
if (( free_gb < NEED_GB )); then
  printf '\033[31mOnly %s GB free; the build needs about %s GB.\033[0m\n\n' "$free_gb" "$NEED_GB"
  say "Safe to clear (caches that rebuild on demand):"
  for d in \
    "$HOME/Library/Caches/Homebrew" \
    "$HOME/Library/Developer/Xcode/DerivedData" \
    "$HOME/Library/Caches/com.apple.dt.Xcode" \
    "$HOME/.bun/install/cache" \
    "$HOME/.npm/_cacache" \
    "$HOME/Library/Caches/pip" \
    "$HOME/anaconda3/pkgs" "$HOME/miniconda3/pkgs" "$HOME/opt/anaconda3/pkgs" \
    "$TARGET"; do
    [[ -d "$d" ]] && printf '  %8s  %s\n' "$(du -sh "$d" 2>/dev/null | cut -f1)" "$d"
  done
  echo
  echo "  brew cleanup --prune=all"
  echo "  rm -rf ~/Library/Developer/Xcode/DerivedData"
  echo "  bun pm cache rm"
  echo "  xcrun simctl delete unavailable      # old iOS simulators"
  echo "  conda clean --all -y                  # if you use conda"
  exit 1
fi
say "$free_gb GB free. Building…"

# --- signing identity -------------------------------------------------------
# Release: a "Developer ID Application" certificate from your Apple Developer
# account, plus notarization credentials stored in the keychain. Checked
# before the build so a missing piece costs seconds, not a full compile.
#
# Local: macOS ties Accessibility and Input Monitoring to an app's signature,
# and Tauri's ad-hoc signature changes on every build, so each rebuild looked
# like a new app and lost its permissions. A self-signed certificate made on
# this Mac gives every build the same identity. It never leaves this Mac and
# can't notarize anything.
SIGN_NAME="L.O.B.A Local Signing"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

ensure_identity() {
  security find-certificate -c "$SIGN_NAME" "$KEYCHAIN" >/dev/null 2>&1 && return 0
  say "Creating the local signing certificate (one time)…"
  local tmp; tmp="$(mktemp -d)"
  # /usr/bin/openssl is LibreSSL; its .p12 imports cleanly into the keychain,
  # which Homebrew's OpenSSL 3 output does not.
  /usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
    -subj "/CN=$SIGN_NAME" \
    -addext "basicConstraints=critical,CA:false" \
    -addext "keyUsage=critical,digitalSignature" \
    -addext "extendedKeyUsage=critical,codeSigning" \
    -keyout "$tmp/key.pem" -out "$tmp/cert.pem" >/dev/null 2>&1 &&
  /usr/bin/openssl pkcs12 -export -inkey "$tmp/key.pem" -in "$tmp/cert.pem" \
    -name "$SIGN_NAME" -passout pass:loba -out "$tmp/id.p12" >/dev/null 2>&1 &&
  security import "$tmp/id.p12" -k "$KEYCHAIN" -P loba -T /usr/bin/codesign >/dev/null
  local ok=$?
  rm -rf "$tmp"
  return $ok
}

if (( RELEASE )); then
  SIGN_WITH="${LOBA_DEVELOPER_ID:-$(security find-identity -v -p codesigning 2>/dev/null |
    sed -n 's/.*"\(Developer ID Application: [^"]*\)".*/\1/p' | head -1)}"
  [[ -n "$SIGN_WITH" ]] || fail "No \"Developer ID Application\" certificate in your keychain.
See LOBA.md > Release for the one-time setup."
  xcrun notarytool history --keychain-profile "$NOTARY_PROFILE" >/dev/null 2>&1 ||
    fail "No notarization credentials saved as \"$NOTARY_PROFILE\".
Run once:  xcrun notarytool store-credentials $NOTARY_PROFILE --apple-id YOU@EXAMPLE.COM --team-id TEAMID
(it asks for an app-specific password from account.apple.com)."
  say "Release build, signing as: $SIGN_WITH"
else
  SIGN_WITH="-"
  if ensure_identity; then
    SIGN_WITH="$SIGN_NAME"
  else
    printf '\033[33mCould not create the signing certificate; using ad-hoc signing.\nPermissions will need granting again after each rebuild.\033[0m\n'
  fi
fi

# Submit a file to Apple and wait. Prints Apple's log and stops if rejected.
notarize() {
  local file="$1" out id
  say "Notarizing $(basename "$file") (usually 1 to 5 minutes)…"
  out=$(xcrun notarytool submit "$file" --keychain-profile "$NOTARY_PROFILE" --wait 2>&1) || true
  printf '%s\n' "$out" | tail -4
  if ! printf '%s\n' "$out" | grep -q "status: Accepted"; then
    id=$(printf '%s\n' "$out" | sed -n 's/^ *id: //p' | head -1)
    [[ -n "$id" ]] && xcrun notarytool log "$id" --keychain-profile "$NOTARY_PROFILE" || true
    fail "Apple did not accept $(basename "$file"); the log above says which file and why."
  fi
}

# --- build -----------------------------------------------------------------
bun install --frozen-lockfile
start=$(date +%s)
CMAKE_POLICY_VERSION_MINIMUM=3.5 \
CARGO_PROFILE_RELEASE_LTO=false \
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
  bun run tauri build --bundles app

app=$(ls -d "$TARGET"/release/bundle/macos/*.app 2>/dev/null | head -1 || true)
[[ -n "$app" ]] || fail "Build finished but no .app was produced; see the output above."

if (( RELEASE )); then
  # Sign inside-out, every Mach-O file on its own: --deep skips libraries
  # kept under Resources, and notarization rejects any unsigned binary.
  # Hardened runtime and a secure timestamp on each are Apple's requirements.
  say "Signing every binary in the app…"
  main_exe="$app/Contents/MacOS/$(/usr/libexec/PlistBuddy -c "Print :CFBundleExecutable" "$app/Contents/Info.plist")"
  while IFS= read -r f; do
    [[ "$f" == "$main_exe" ]] && continue
    file -b "$f" | grep -q "Mach-O" || continue
    codesign --force --options runtime --timestamp --sign "$SIGN_WITH" "$f"
  done < <(find "$app/Contents" -type f | awk '{ print gsub("/", "/"), $0 }' | sort -rn | cut -d' ' -f2-)
  codesign --force --options runtime --timestamp --entitlements src-tauri/Entitlements.plist \
    --sign "$SIGN_WITH" "$app"
  codesign --verify --deep --strict --verbose=2 "$app" || fail "The signed app didn't verify."

  appzip="$(mktemp -d)/L.O.B.A.zip"
  ditto -c -k --keepParent "$app" "$appzip"
  notarize "$appzip"
  rm -f "$appzip"
  xcrun stapler staple "$app" || fail "Couldn't staple the notarization ticket to the app."
elif [[ "$SIGN_WITH" != "-" ]]; then
  say "Signing with \"$SIGN_NAME\"…"
  # The first time, macOS asks whether codesign may use the key: choose
  # "Always Allow" so later builds don't ask again.
  ent="$(mktemp)"
  cp src-tauri/Entitlements.plist "$ent"
  # A self-signed build has no Team ID, so the hardened runtime's library
  # check would refuse the app's own bundled libraries.
  /usr/libexec/PlistBuddy -c "Add :com.apple.security.cs.disable-library-validation bool true" "$ent" >/dev/null 2>&1 || true
  if ! codesign --force --deep --options runtime --entitlements "$ent" --sign "$SIGN_NAME" "$app"; then
    printf '\033[33mSigning with the local certificate failed (message above); falling back to ad-hoc.\033[0m\n'
    SIGN_WITH="-"
    codesign --force --deep --options runtime --entitlements src-tauri/Entitlements.plist --sign - "$app"
  fi
  rm -f "$ent"
  codesign --verify --deep --strict "$app" || fail "The signed app didn't verify."
fi

# DMG built here rather than by Tauri, so it holds the app as signed above.
version=$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "$app/Contents/Info.plist")
name=$(basename "$app" .app)
stage="$(mktemp -d)"
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
out="$HOME/Downloads/${name}_${version}_aarch64.dmg"
rm -f "$out"
hdiutil create -quiet -volname "$name" -srcfolder "$stage" -ov -format UDZO "$out" ||
  fail "Couldn't create the DMG."
rm -rf "$stage"

if (( RELEASE )); then
  codesign --force --timestamp --sign "$SIGN_WITH" "$out"
  notarize "$out"
  xcrun stapler staple "$out" || fail "Couldn't staple the notarization ticket to the DMG."
  # What Gatekeeper will say on someone else's Mac.
  spctl -a -vv -t exec "$app" 2>&1 | sed 's/^/  app: /'
  spctl -a -vv -t open --context context:primary-signature "$out" 2>&1 | sed 's/^/  dmg: /'
fi
say "DMG: $out  ($(( ($(date +%s) - start) / 60 )) min, signed: $SIGN_WITH$( (( RELEASE )) && echo ', notarized'))"

# --- clean up --------------------------------------------------------------
if (( KEEP )); then
  say "Kept build tree ($(du -sh "$TARGET" | cut -f1)) for faster rebuilds."
else
  size=$(du -sh "$TARGET" | cut -f1)
  rm -rf "$TARGET"
  say "Removed build tree, freed $size. (Use --keep to keep it next time.)"
fi
