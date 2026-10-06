#!/usr/bin/env bash
# Build L.O.B.A.dmg on an Apple Silicon Mac, lean on disk.
#
#   ./scripts/build-mac.sh          build, put the DMG in ~/Downloads, delete the build tree
#   ./scripts/build-mac.sh --keep   same, but keep src-tauri/target for faster rebuilds
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
[[ "${1:-}" == "--keep" ]] && KEEP=1

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
# macOS ties Accessibility and Input Monitoring to an app's signature. Tauri's
# default ad-hoc signature changes on every build, so each rebuild looked like
# a new app and lost its permissions. A local self-signed certificate gives
# every build the same identity. It never leaves this Mac and is only for
# running your own build; it can't notarize or distribute anything.
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

SIGN_WITH="-"
if ensure_identity; then
  SIGN_WITH="$SIGN_NAME"
else
  printf '\033[33mCould not create the signing certificate; using ad-hoc signing.\nPermissions will need granting again after each rebuild.\033[0m\n'
fi

# --- build -----------------------------------------------------------------
bun install --frozen-lockfile
start=$(date +%s)
CMAKE_POLICY_VERSION_MINIMUM=3.5 \
CARGO_PROFILE_RELEASE_LTO=false \
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
  bun run tauri build --bundles app

app=$(ls -d "$TARGET"/release/bundle/macos/*.app 2>/dev/null | head -1 || true)
[[ -n "$app" ]] || fail "Build finished but no .app was produced; see the output above."

if [[ "$SIGN_WITH" != "-" ]]; then
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
say "DMG: $out  ($(( ($(date +%s) - start) / 60 )) min, signed: $SIGN_WITH)"

# --- clean up --------------------------------------------------------------
if (( KEEP )); then
  say "Kept build tree ($(du -sh "$TARGET" | cut -f1)) for faster rebuilds."
else
  size=$(du -sh "$TARGET" | cut -f1)
  rm -rf "$TARGET"
  say "Removed build tree, freed $size. (Use --keep to keep it next time.)"
fi
