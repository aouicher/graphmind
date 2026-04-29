#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
DESKTOP_DIR="$ROOT_DIR/crates/graphmind-desktop"

echo ">> Building GraphMind Desktop..."

# Build frontend
cd "$DESKTOP_DIR"
npm run build

# Build Tauri app
cd "$DESKTOP_DIR"
tauri build

# If tauri build DMG fails, create manually
DMG_PATH="$ROOT_DIR/target/release/bundle/dmg/GraphMind_0.1.0_aarch64.dmg"
APP_PATH="$ROOT_DIR/target/release/bundle/macos/GraphMind.app"

if [ ! -f "$DMG_PATH" ] && [ -d "$APP_PATH" ]; then
    echo ">> Creating DMG manually..."
    mkdir -p "$(dirname "$DMG_PATH")"
    hdiutil create -volname "GraphMind" \
        -srcfolder "$APP_PATH" \
        -ov -format UDZO \
        "$DMG_PATH"
fi

echo ""
echo ">> Done!"
echo "   App: $APP_PATH"
echo "   DMG: $DMG_PATH"
du -sh "$DMG_PATH" 2>/dev/null
