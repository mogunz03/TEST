#!/usr/bin/env bash
# Build L.O.B.A.dmg on an Apple Silicon Mac, lean on disk.
#
#   ./scripts/build-mac.sh          build, copy the DMG to ~/Downloads, delete the build tree
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

# --- build -----------------------------------------------------------------
bun install --frozen-lockfile
start=$(date +%s)
CMAKE_POLICY_VERSION_MINIMUM=3.5 \
CARGO_PROFILE_RELEASE_LTO=false \
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
  bun run tauri build --bundles app,dmg

dmg=$(ls -t "$TARGET"/release/bundle/dmg/*.dmg 2>/dev/null | head -1 || true)
[[ -n "$dmg" ]] || fail "Build finished but no DMG was produced; see the output above."
out="$HOME/Downloads/$(basename "$dmg")"
cp -f "$dmg" "$out"
say "DMG: $out  ($(( ($(date +%s) - start) / 60 )) min)"

# --- clean up --------------------------------------------------------------
if (( KEEP )); then
  say "Kept build tree ($(du -sh "$TARGET" | cut -f1)) for faster rebuilds."
else
  size=$(du -sh "$TARGET" | cut -f1)
  rm -rf "$TARGET"
  say "Removed build tree, freed $size. (Use --keep to keep it next time.)"
fi
