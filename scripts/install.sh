#!/bin/zsh
set -euo pipefail
PROJECT_DIR="${0:A:h:h}"
cd "$PROJECT_DIR"
INSTALL_DIR="${RAMPLE_INSTALL_DIR:-/Applications}"
APP_NAME="Rample Preview.app"
INSTALL_APP="$INSTALL_DIR/$APP_NAME"
VERSION="$(cat VERSION | tr -d '\n')"

npm run tauri -- build --bundles app
BUILD_APP="$PROJECT_DIR/target/release/bundle/macos/$APP_NAME"
codesign --verify --deep --strict "$BUILD_APP"
mkdir -p "$INSTALL_DIR" "$PROJECT_DIR/.build/legacy"
STAGED_APP="$INSTALL_DIR/.RamplePreview-install-$$.app"
BACKUP_APP="$INSTALL_DIR/.RamplePreview-previous-$$.app"
ditto "$BUILD_APP" "$STAGED_APP"

# Build first, then close the installed app so it can save before replacement.
if pgrep -x RamplePreview >/dev/null; then
    osascript - "$INSTALL_APP" <<'APPLESCRIPT'
on run argv
    tell application (item 1 of argv) to quit
end run
APPLESCRIPT
    for i in {1..50}; do
        if ! pgrep -x RamplePreview >/dev/null; then break; fi
        sleep 0.1
    done
    if pgrep -x RamplePreview >/dev/null; then
        print -u2 'The app is still running. Installation cancelled; the installed version is untouched.'
        rm -rf "$STAGED_APP"
        exit 1
    fi
fi
if [[ -d "$INSTALL_APP" ]]; then
    OLD_VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$INSTALL_APP/Contents/Info.plist")"
    if [[ "$OLD_VERSION" == 1.* && ! -d "$PROJECT_DIR/.build/legacy/Rample Preview $OLD_VERSION.app" ]]; then
        ditto "$INSTALL_APP" "$PROJECT_DIR/.build/legacy/Rample Preview $OLD_VERSION.app"
    fi
    mv "$INSTALL_APP" "$BACKUP_APP"
fi
if ! mv "$STAGED_APP" "$INSTALL_APP"; then
    if [[ -d "$BACKUP_APP" ]]; then mv "$BACKUP_APP" "$INSTALL_APP"; fi
    exit 1
fi
if [[ -d "$BACKUP_APP" ]]; then rm -rf "$BACKUP_APP"; fi
print "Installed: $INSTALL_APP (v$VERSION)"
if [[ "${1:-}" != "--no-launch" ]]; then open "$INSTALL_APP"; fi
