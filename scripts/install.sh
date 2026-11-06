#!/bin/zsh
set -euo pipefail
# Bundling requires Apple tools, not Python packages with the same names.
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
PROJECT_DIR="${0:A:h:h}"
cd "$PROJECT_DIR"
INSTALL_DIR="${RAMPLE_INSTALL_DIR:-/Applications}"
APP_NAME="Rample Manager.app"
INSTALL_APP="$INSTALL_DIR/$APP_NAME"
# Before 3.0 the app was called Rample Preview.
PREVIOUS_APP="$INSTALL_DIR/Rample Preview.app"
VERSION="$(cat VERSION | tr -d '\n')"

npm run tauri -- build --bundles app
BUILD_APP="$PROJECT_DIR/target/release/bundle/macos/$APP_NAME"
codesign --verify --deep --strict "$BUILD_APP"
mkdir -p "$INSTALL_DIR" "$PROJECT_DIR/.build/legacy"
STAGED_APP="$INSTALL_DIR/.RampleManager-install-$$.app"
BACKUP_APP="$INSTALL_DIR/.RampleManager-previous-$$.app"
ditto "$BUILD_APP" "$STAGED_APP"

# Build first, then close the installed app so it can save before replacement.
quit_app() {
    local process="$1" bundle="$2"
    pgrep -x "$process" >/dev/null || return 0
    osascript - "$bundle" <<'APPLESCRIPT'
on run argv
    tell application (item 1 of argv) to quit
end run
APPLESCRIPT
    for i in {1..50}; do
        if ! pgrep -x "$process" >/dev/null; then return 0; fi
        sleep 0.1
    done
    print -u2 'The app is still running. Installation cancelled; the installed version is untouched.'
    rm -rf "$STAGED_APP"
    exit 1
}
quit_app RampleManager "$INSTALL_APP"
quit_app RamplePreview "$PREVIOUS_APP"

# Keep a local backup of an installed app under its earlier name or version.
backup_legacy() {
    local app="$1" name="$2"
    [[ -d "$app" ]] || return 0
    local version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$app/Contents/Info.plist")"
    local backup="$PROJECT_DIR/.build/legacy/$name $version.app"
    if [[ ! -d "$backup" ]]; then ditto "$app" "$backup"; fi
}
if [[ -d "$PREVIOUS_APP" ]]; then
    backup_legacy "$PREVIOUS_APP" "Rample Preview"
fi
if [[ -d "$INSTALL_APP" ]]; then
    mv "$INSTALL_APP" "$BACKUP_APP"
fi
if ! mv "$STAGED_APP" "$INSTALL_APP"; then
    if [[ -d "$BACKUP_APP" ]]; then mv "$BACKUP_APP" "$INSTALL_APP"; fi
    exit 1
fi
if [[ -d "$BACKUP_APP" ]]; then rm -rf "$BACKUP_APP"; fi
if [[ -d "$PREVIOUS_APP" ]]; then rm -rf "$PREVIOUS_APP"; fi
print "Installed: $INSTALL_APP (v$VERSION)"
if [[ "${1:-}" != "--no-launch" ]]; then open "$INSTALL_APP"; fi
