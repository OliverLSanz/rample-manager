#!/bin/zsh
# Publishes site/ (landing page and manual) to https://oliverlsanz.com/RampleManager/.
# The VPS's Nginx serves ~/vps/html/RampleManager; see "Website" in README.md.
set -euo pipefail
PROJECT_DIR="${0:A:h:h}"
HOST="${RAMPLE_SITE_HOST:-oliverlsanz.com}"
REMOTE_DIR="${RAMPLE_SITE_DIR:-vps/html/RampleManager}"
URL="${RAMPLE_SITE_URL:-https://oliverlsanz.com/RampleManager/}"

ssh "$HOST" "mkdir -p '$REMOTE_DIR'"
rsync -rlz --delete --chmod=D755,F644 --exclude .DS_Store "$PROJECT_DIR/site/" "$HOST:$REMOTE_DIR/"
for page in "" manual/ assets/style.css assets/site.js assets/screens/main.webp; do
    curl --fail --silent --show-error --output /dev/null "$URL$page"
done
print "Published: $URL"
