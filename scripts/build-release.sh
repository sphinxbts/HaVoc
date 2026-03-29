#!/usr/bin/env bash
# Build HVOC release binaries and package them for distribution.
#
# Usage:
#   bash scripts/build-release.sh
#
# Environment:
#   TARGET  — override target triple (auto-detected if not set)
#   MACOS_APP — set to "1" to also create .app bundle + DMG (macOS only)
#
# Output:
#   dist/hvoc-<target>.zip   — binary + frontend + README
#   HaVoc.app                — macOS app bundle (if MACOS_APP=1)
#   HaVoc-*.dmg              — macOS DMG installer (if MACOS_APP=1)

set -euo pipefail

echo "=== HVOC Release Build ==="

# Detect target triple
TARGET="${TARGET:-$(rustc -vV | grep '^host:' | awk '{print $2}')}"
echo "Target: $TARGET"

# Build release binary
echo ""
echo "Building release binary..."
cargo build --release --target "$TARGET" --package hvoc-cli

# Determine binary name
if [[ "$TARGET" == *"windows"* ]]; then
  BIN="target/$TARGET/release/hvoc-cli.exe"
else
  BIN="target/$TARGET/release/hvoc-cli"
fi

if [[ ! -f "$BIN" ]]; then
  # Fall back to default target directory
  if [[ "$TARGET" == *"windows"* ]]; then
    BIN="target/release/hvoc-cli.exe"
  else
    BIN="target/release/hvoc-cli"
  fi
fi

if [[ ! -f "$BIN" ]]; then
  echo "ERROR: binary not found"
  exit 1
fi

SIZE=$(du -h "$BIN" | awk '{print $1}')
echo "Binary built: $BIN ($SIZE)"

# Create distribution directory
DIST_DIR="dist"
STAGE_DIR="$DIST_DIR/hvoc-$TARGET"
rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR"

# Copy files
cp "$BIN" "$STAGE_DIR/"
cp hvoc.html "$STAGE_DIR/"
cp README.md "$STAGE_DIR/"

# Create archive
echo ""
echo "Packaging..."
cd "$DIST_DIR"
if command -v zip >/dev/null 2>&1; then
  zip -r "hvoc-$TARGET.zip" "hvoc-$TARGET/"
  ARCHIVE="dist/hvoc-$TARGET.zip"
elif command -v tar >/dev/null 2>&1; then
  tar czf "hvoc-$TARGET.tar.gz" "hvoc-$TARGET/"
  ARCHIVE="dist/hvoc-$TARGET.tar.gz"
else
  echo "WARNING: neither zip nor tar found; files staged in $STAGE_DIR"
  ARCHIVE="$STAGE_DIR"
fi
cd ..

echo ""
echo "=== Build Complete ==="
echo "Binary:  $BIN"
echo "Package: $ARCHIVE"

# ── macOS .app bundle + DMG ──
if [[ "${MACOS_APP:-0}" == "1" && "$TARGET" == *"apple-darwin"* ]]; then
  echo ""
  echo "=== Creating macOS .app bundle ==="

  VERSION=$(grep '^version' hvoc-cli/Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
  bash scripts/create-macos-app.sh "$BIN" hvoc.html "$VERSION"

  echo ""
  echo "=== Creating DMG installer ==="
  bash scripts/create-dmg.sh HaVoc.app "HaVoc-v${VERSION}-macOS-$(uname -m).dmg"
fi

echo ""
echo "To run:"
echo "  1. Extract the archive"
echo "  2. Run: ./hvoc-cli serve"
echo "  3. Open hvoc.html in a browser"
