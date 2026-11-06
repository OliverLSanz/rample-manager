#!/bin/zsh
# Publishes site/ (landing page and manual) to https://oliverlsanz.com/RampleManager/.
# The VPS's Nginx serves ~/vps/html/RampleManager; see "Website" in docs/development.md.
set -euo pipefail
PROJECT_DIR="${0:A:h:h}"
HOST="${RAMPLE_SITE_HOST:-oliverlsanz.com}"
REMOTE_DIR="${RAMPLE_SITE_DIR:-vps/html/RampleManager}"
URL="${RAMPLE_SITE_URL:-https://oliverlsanz.com/RampleManager/}"

ssh "$HOST" "mkdir -p '$REMOTE_DIR'"
# macOS ships openrsync, without --chmod: set permissions remotely instead.
rsync -rlz --delete --exclude .DS_Store "$PROJECT_DIR/site/" "$HOST:$REMOTE_DIR/"
ssh "$HOST" "find '$REMOTE_DIR' -type d -exec chmod 755 {} + && find '$REMOTE_DIR' -type f -exec chmod 644 {} +"
for page in "" manual/ assets/style.css assets/site.js assets/screens/main.webp assets/video/tour.mp4; do
    curl --fail --silent --show-error --output /dev/null "$URL$page"
done
print "Published: $URL"
