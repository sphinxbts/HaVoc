#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# create-macos-app.sh — Package hvoc-cli into a proper macOS .app bundle
#
# Usage:
#   bash scripts/create-macos-app.sh <binary> <html-frontend> [version]
#
# Example:
#   bash scripts/create-macos-app.sh target/release/hvoc-cli hvoc.html v0.1.0
#
# Output:
#   HaVoc.app/                     ← standard macOS application bundle
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

BINARY="${1:?Usage: $0 <binary> <html-frontend> [version]}"
HTML_FRONTEND="${2:?Usage: $0 <binary> <html-frontend> [version]}"
VERSION="${3:-0.1.0}"

# Strip leading 'v' from version if present
VERSION="${VERSION#v}"

APP_NAME="HaVoc"
BUNDLE_ID="com.hvoc.havoc"
APP_DIR="${APP_NAME}.app"

echo "=== Creating macOS .app bundle ==="
echo "Binary:   $BINARY"
echo "Frontend: $HTML_FRONTEND"
echo "Version:  $VERSION"
echo ""

# ── Clean & scaffold ──
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"

# ── Copy binary ──
cp "$BINARY" "$APP_DIR/Contents/MacOS/hvoc-cli"
chmod +x "$APP_DIR/Contents/MacOS/hvoc-cli"

# ── Copy frontend HTML ──
cp "$HTML_FRONTEND" "$APP_DIR/Contents/Resources/hvoc.html"

# ── Create launcher script ──
# This script launches hvoc-cli serve and opens the frontend
cat > "$APP_DIR/Contents/MacOS/HaVoc" << 'LAUNCHER'
#!/usr/bin/env bash
# HaVoc macOS launcher
# Starts the hvoc-cli server and opens the web UI

DIR="$(cd "$(dirname "$0")" && pwd)"
RESOURCES="$DIR/../Resources"
LOG_DIR="$HOME/.hvoc/logs"
mkdir -p "$LOG_DIR"

# Kill any existing hvoc-cli processes on our port
lsof -ti:7734 2>/dev/null | xargs kill -9 2>/dev/null || true

# Start the server in the background
"$DIR/hvoc-cli" serve --bind 127.0.0.1:7734 \
  > "$LOG_DIR/hvoc.log" 2>&1 &
SERVER_PID=$!

# Wait for the server to be ready (max 15 seconds)
echo "Starting HaVoc server..."
for i in $(seq 1 30); do
  if curl -s http://127.0.0.1:7734/api/identity >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done

# Open the frontend in default browser
if [ -f "$RESOURCES/hvoc.html" ]; then
  open "$RESOURCES/hvoc.html"
else
  open "http://127.0.0.1:7734"
fi

# Keep the app "running" while the server is alive
# This way Cmd+Q will trigger the trap below
cleanup() {
  kill "$SERVER_PID" 2>/dev/null || true
  exit 0
}
trap cleanup SIGTERM SIGINT SIGHUP

wait "$SERVER_PID"
LAUNCHER
chmod +x "$APP_DIR/Contents/MacOS/HaVoc"

# ── Generate Info.plist ──
cat > "$APP_DIR/Contents/Info.plist" << PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>

    <key>CFBundleDisplayName</key>
    <string>HaVoc — Veilid P2P Chat</string>

    <key>CFBundleIdentifier</key>
    <string>${BUNDLE_ID}</string>

    <key>CFBundleVersion</key>
    <string>${VERSION}</string>

    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>

    <key>CFBundleExecutable</key>
    <string>HaVoc</string>

    <key>CFBundlePackageType</key>
    <string>APPL</string>

    <key>CFBundleSignature</key>
    <string>HVOC</string>

    <key>CFBundleIconFile</key>
    <string>AppIcon</string>

    <key>LSMinimumSystemVersion</key>
    <string>14.0</string>

    <key>NSHighResolutionCapable</key>
    <true/>

    <key>LSUIElement</key>
    <false/>

    <key>NSSupportsAutomaticTermination</key>
    <true/>

    <key>NSSupportsSuddenTermination</key>
    <false/>

    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>

    <key>NSHumanReadableCopyright</key>
    <string>HaVoc — HVCK's Veilid Overlay Chat</string>

    <key>LSApplicationCategoryType</key>
    <string>public.app-category.social-networking</string>

    <key>NSAppTransportSecurity</key>
    <dict>
        <key>NSAllowsLocalNetworking</key>
        <true/>
    </dict>
</dict>
</plist>
PLIST

# ── Generate a simple icon (if iconutil is available) ──
generate_icon() {
  # Create a simple SVG-based icon using sips (macOS built-in)
  # If on CI without graphical tools, skip icon generation
  if ! command -v sips &>/dev/null; then
    echo "sips not available — skipping icon generation"
    return
  fi

  ICONSET_DIR=$(mktemp -d)/AppIcon.iconset
  mkdir -p "$ICONSET_DIR"

  # Create a simple colored square as placeholder icon using Python
  python3 - "$ICONSET_DIR" << 'PYICON' || true
import sys, struct, zlib

iconset_dir = sys.argv[1]

def create_png(width, height, r, g, b):
    """Create a minimal PNG with a solid color."""
    def make_chunk(chunk_type, data):
        c = chunk_type + data
        return struct.pack('>I', len(data)) + c + struct.pack('>I', zlib.crc32(c) & 0xffffffff)

    header = b'\x89PNG\r\n\x1a\n'
    ihdr = make_chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0))

    raw_data = b''
    for y in range(height):
        raw_data += b'\x00'  # filter byte
        for x in range(width):
            # Create a gradient effect
            fr = min(255, int(r * (1.0 - 0.3 * y / height)))
            fg = min(255, int(g * (1.0 - 0.2 * y / height)))
            fb = min(255, int(b * (0.7 + 0.3 * x / width)))
            raw_data += struct.pack('BBB', fr, fg, fb)

    idat = make_chunk(b'IDAT', zlib.compress(raw_data))
    iend = make_chunk(b'IEND', b'')
    return header + ihdr + idat + iend

# Deep purple/blue gradient — matches HaVoc's dark theme
sizes = {
    'icon_16x16.png': 16,
    'icon_16x16@2x.png': 32,
    'icon_32x32.png': 32,
    'icon_32x32@2x.png': 64,
    'icon_128x128.png': 128,
    'icon_128x128@2x.png': 256,
    'icon_256x256.png': 256,
    'icon_256x256@2x.png': 512,
    'icon_512x512.png': 512,
    'icon_512x512@2x.png': 1024,
}

for name, size in sizes.items():
    png = create_png(size, size, 60, 20, 180)
    with open(f"{iconset_dir}/{name}", 'wb') as f:
        f.write(png)

print(f"Generated {len(sizes)} icon sizes")
PYICON

  # Convert iconset to .icns
  if command -v iconutil &>/dev/null && [ -d "$ICONSET_DIR" ]; then
    iconutil -c icns "$ICONSET_DIR" -o "$APP_DIR/Contents/Resources/AppIcon.icns"
    echo "Icon created: AppIcon.icns"
  fi

  rm -rf "$(dirname "$ICONSET_DIR")"
}

generate_icon

# ── PkgInfo ──
echo -n "APPLHVOC" > "$APP_DIR/Contents/PkgInfo"

# ── Summary ──
echo ""
echo "=== .app bundle created ==="
echo "Location: $APP_DIR"
echo ""
du -sh "$APP_DIR"
echo ""
echo "Contents:"
find "$APP_DIR" -type f | sort | while read -r f; do
  size=$(du -h "$f" | awk '{print $1}')
  echo "  $size  $f"
done
echo ""
echo "To run (after removing quarantine):"
echo "  xattr -cr $APP_DIR && open $APP_DIR"
