#!/usr/bin/env bash
#
# build-release-macos.sh — Build CrateBay v0.1.0 macOS release artifacts
#
# Produces:
#   dist/CrateBay_<version>_<arch>.dmg  — GUI app with embedded daemon
#   dist/cratebay                       — CLI binary
#
# Usage:
#   ./scripts/build-release-macos.sh
#
set -euo pipefail

# Ensure Cargo/Rust toolchain is on PATH
if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

VERSION="0.1.0"
ARCH="$(uname -m)"                           # aarch64 or x86_64
RUST_TARGET="$(rustc -vV | grep host | awk '{print $2}')"  # e.g. aarch64-apple-darwin

GUI_CRATE="crates/cratebay-gui"
TAURI_DIR="$GUI_CRATE/src-tauri"

echo "=== CrateBay macOS Release Build ==="
echo "  Version : $VERSION"
echo "  Arch    : $ARCH"
echo "  Target  : $RUST_TARGET"
echo ""

# ── Step 1: Build daemon & CLI ───────────────────────────────────────────────
echo "── [1/6] Building daemon and CLI (release) ──"
cargo build --release -p cratebay-daemon -p cratebay-cli

echo "  ✓ target/release/cratebay-daemon"
echo "  ✓ target/release/cratebay"

# ── Step 2: Install frontend dependencies ────────────────────────────────────
echo ""
echo "── [2/6] Installing frontend dependencies ──"
(cd "$GUI_CRATE" && npm ci)

# ── Step 3: Build Tauri app ──────────────────────────────────────────────────
echo ""
echo "── [3/6] Building Tauri app ──"
(cd "$GUI_CRATE" && npx tauri build)

# Locate the .app bundle produced by Tauri
# Workspace builds place bundles under the workspace root target/ directory
BUNDLE_DIR="target/release/bundle/macos"
APP_BUNDLE="$(find "$BUNDLE_DIR" -name '*.app' -maxdepth 1 | head -1)"
if [ -z "$APP_BUNDLE" ]; then
    echo "ERROR: Could not find .app bundle under $BUNDLE_DIR"
    exit 1
fi
APP_NAME="$(basename "$APP_BUNDLE")"
echo "  ✓ $APP_BUNDLE"

# ── Step 4: Inject daemon into .app bundle ───────────────────────────────────
echo ""
echo "── [4/6] Injecting daemon into $APP_NAME ──"
cp "target/release/cratebay-daemon" "$APP_BUNDLE/Contents/MacOS/cratebay-daemon"
echo "  ✓ $APP_BUNDLE/Contents/MacOS/cratebay-daemon"

# Verify bundle structure
echo ""
echo "  Bundle Contents/MacOS/:"
ls -la "$APP_BUNDLE/Contents/MacOS/"

# ── Step 5: Rebuild DMG ──────────────────────────────────────────────────────
echo ""
echo "── [5/6] Creating DMG ──"
DIST_DIR="$REPO_ROOT/dist"
mkdir -p "$DIST_DIR"

DMG_NAME="CrateBay_${VERSION}_${ARCH}.dmg"
DMG_PATH="$DIST_DIR/$DMG_NAME"

# Remove old DMG if present
rm -f "$DMG_PATH"

# Create a temporary directory for DMG contents
DMG_STAGE="$(mktemp -d)"
cp -R "$APP_BUNDLE" "$DMG_STAGE/"

# Add a symlink to /Applications for drag-to-install
ln -s /Applications "$DMG_STAGE/Applications"

hdiutil create \
    -volname "CrateBay $VERSION" \
    -srcfolder "$DMG_STAGE" \
    -ov \
    -format UDZO \
    "$DMG_PATH"

rm -rf "$DMG_STAGE"
echo "  ✓ $DMG_PATH"

# ── Step 6: Collect CLI binary ───────────────────────────────────────────────
echo ""
echo "── [6/6] Collecting CLI binary ──"
cp "target/release/cratebay" "$DIST_DIR/cratebay"
echo "  ✓ $DIST_DIR/cratebay"

# ── Summary ──────────────────────────────────────────────────────────────────
echo ""
echo "=== Build Complete ==="
echo ""
echo "Artifacts:"
echo "  GUI (DMG): $DMG_PATH"
echo "  CLI:       $DIST_DIR/cratebay"
echo ""
echo "DMG size: $(du -h "$DMG_PATH" | awk '{print $1}')"
echo "CLI size: $(du -h "$DIST_DIR/cratebay" | awk '{print $1}')"
echo ""
echo "Next steps:"
echo "  1. Open the DMG and drag CrateBay to Applications"
echo "  2. Launch CrateBay from Applications"
echo "  3. Test: ./dist/cratebay status"
