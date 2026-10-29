#!/bin/zsh
set -euo pipefail

PROJECT_DIR="${0:A:h:h}"
cd "$PROJECT_DIR"
VERSION="$(cat VERSION | tr -d '\n')"
APP_NAME="Rample Preview.app"
BUILD_APP="$PROJECT_DIR/.build/$APP_NAME"
INSTALL_DIR="${RAMPLE_INSTALL_DIR:-/Applications}"
INSTALL_APP="$INSTALL_DIR/$APP_NAME"

swift build -c release
BIN_DIR="$(swift build -c release --show-bin-path)"
mkdir -p "$BUILD_APP/Contents/MacOS" "$BUILD_APP/Contents/Resources"
cp "$BIN_DIR/RamplePreview" "$BUILD_APP/Contents/MacOS/RamplePreview"
cp Resources/Info.plist "$BUILD_APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" "$BUILD_APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $(date +%Y%m%d%H%M%S)" "$BUILD_APP/Contents/Info.plist"

ICON_DIR="$PROJECT_DIR/.build/AppIcon.iconset"
mkdir -p "$ICON_DIR"
swift scripts/icon.swift "$PROJECT_DIR/.build/icon-1024.png"
for PIXELS in 16 32 128 256 512; do
    sips -z "$PIXELS" "$PIXELS" "$PROJECT_DIR/.build/icon-1024.png" --out "$ICON_DIR/icon_${PIXELS}x${PIXELS}.png" >/dev/null
    DOUBLE=$(( PIXELS * 2 ))
    sips -z "$DOUBLE" "$DOUBLE" "$PROJECT_DIR/.build/icon-1024.png" --out "$ICON_DIR/icon_${PIXELS}x${PIXELS}@2x.png" >/dev/null
done
iconutil -c icns "$ICON_DIR" -o "$BUILD_APP/Contents/Resources/AppIcon.icns"
codesign --force --deep --sign - --identifier net.oliver.rample-preview "$BUILD_APP"
codesign --verify --deep --strict "$BUILD_APP"

mkdir -p "$INSTALL_DIR"
STAGED_APP="$INSTALL_DIR/.RamplePreview-install-$$.app"
BACKUP_APP="$INSTALL_DIR/.RamplePreview-previous-$$.app"
ditto "$BUILD_APP" "$STAGED_APP"
if pgrep -x RamplePreview >/dev/null; then
    osascript -e 'tell application id "net.oliver.rample-preview" to quit'
    for i in {1..30}; do
        if ! pgrep -x RamplePreview >/dev/null; then break; fi
        sleep 0.1
    done
    if pgrep -x RamplePreview >/dev/null; then
        print -u2 'The app is still running; installation cancelled to preserve the session.'
        rm -rf "$STAGED_APP"
        exit 1
    fi
fi
if [[ -d "$INSTALL_APP" ]]; then mv "$INSTALL_APP" "$BACKUP_APP"; fi
if ! mv "$STAGED_APP" "$INSTALL_APP"; then
    if [[ -d "$BACKUP_APP" ]]; then mv "$BACKUP_APP" "$INSTALL_APP"; fi
    exit 1
fi
if [[ -d "$BACKUP_APP" ]]; then rm -rf "$BACKUP_APP"; fi
print "Installed: $INSTALL_APP (v$VERSION)"
if [[ "${1:-}" != "--no-launch" ]]; then open "$INSTALL_APP"; fi
