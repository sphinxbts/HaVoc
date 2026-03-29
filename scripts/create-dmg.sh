#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# create-dmg.sh — Create a macOS .dmg installer from a .app bundle
#
# Usage:
#   bash scripts/create-dmg.sh <app-bundle> <output-dmg>
#
# Example:
#   bash scripts/create-dmg.sh HaVoc.app HaVoc-v0.1.0-macOS-Universal.dmg
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

APP_BUNDLE="${1:?Usage: $0 <app-bundle> <output-dmg>}"
OUTPUT_DMG="${2:?Usage: $0 <app-bundle> <output-dmg>}"

APP_NAME="$(basename "$APP_BUNDLE" .app)"
VOL_NAME="$APP_NAME Installer"
DMG_TEMP="pack.temp.dmg"
STAGING_DIR=$(mktemp -d)

echo "=== Creating macOS DMG installer ==="
echo "App:    $APP_BUNDLE"
echo "Output: $OUTPUT_DMG"
echo ""

# ── Verify .app exists ──
if [ ! -d "$APP_BUNDLE" ]; then
  echo "ERROR: $APP_BUNDLE not found"
  exit 1
fi

# ── Stage contents ──
echo "Staging files..."
cp -R "$APP_BUNDLE" "$STAGING_DIR/"

# Create Applications symlink for drag-to-install
ln -s /Applications "$STAGING_DIR/Applications"

# Add a README
cat > "$STAGING_DIR/README.txt" << 'README'
HaVoc — Veilid P2P Forum + Messaging
=====================================

Installation:
  Drag HaVoc.app into the Applications folder.

First Run (important!):
  macOS will block unsigned apps. To open:
  1. Right-click HaVoc.app → "Open" → click "Open" in the dialog
  — OR —
  2. Open Terminal and run:
     xattr -cr /Applications/HaVoc.app

Usage:
  • Double-click HaVoc.app to start
  • The server starts at http://127.0.0.1:7734
  • The web UI opens automatically in your browser
  • Data stored in ~/.hvoc/

CLI Usage:
  /Applications/HaVoc.app/Contents/MacOS/hvoc-cli serve
  /Applications/HaVoc.app/Contents/MacOS/hvoc-cli identity list
  /Applications/HaVoc.app/Contents/MacOS/hvoc-cli thread list

For more info: https://github.com/sphinxbts/HaVoc
README

echo "Creating DMG..."

# ── Create temporary DMG ──
hdiutil create \
  -srcfolder "$STAGING_DIR" \
  -volname "$VOL_NAME" \
  -fs HFS+ \
  -format UDRW \
  "$DMG_TEMP" \
  2>/dev/null

# ── Mount and customise ──
MOUNT_DIR=$(hdiutil attach -readwrite -noverify "$DMG_TEMP" 2>/dev/null | \
  grep "/Volumes/" | awk '{print $NF}')

if [ -n "$MOUNT_DIR" ]; then
  echo "Mounted at: $MOUNT_DIR"

  # Set DMG window appearance via AppleScript (if running interactively)
  # This positions the icons nicely for drag-to-install
  osascript << APPLESCRIPT 2>/dev/null || true
tell application "Finder"
  tell disk "$VOL_NAME"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set bounds of container window to {100, 100, 640, 420}
    set viewOptions to the icon view options of container window
    set arrangement of viewOptions to not arranged
    set icon size of viewOptions to 96
    set position of item "$APP_NAME.app" of container window to {140, 160}
    set position of item "Applications" of container window to {400, 160}
    set position of item "README.txt" of container window to {270, 300}
    close
    open
    update without registering applications
    delay 2
  end tell
end tell
APPLESCRIPT

  # Set custom background color (dark)
  # Unfortunately pure CLI can't easily set background without an image

  # Unmount
  sync
  hdiutil detach "$MOUNT_DIR" 2>/dev/null || true
fi

# ── Convert to compressed read-only DMG ──
echo "Compressing..."
rm -f "$OUTPUT_DMG"
hdiutil convert "$DMG_TEMP" \
  -format UDZO \
  -imagekey zlib-level=9 \
  -o "$OUTPUT_DMG" \
  2>/dev/null

rm -f "$DMG_TEMP"
rm -rf "$STAGING_DIR"

# ── Summary ──
DMG_SIZE=$(du -h "$OUTPUT_DMG" | awk '{print $1}')
echo ""
echo "=== DMG created ==="
echo "File: $OUTPUT_DMG"
echo "Size: $DMG_SIZE"
echo ""
echo "To test locally:"
echo "  open $OUTPUT_DMG"
