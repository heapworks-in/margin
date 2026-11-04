#!/bin/sh
# Screenshots for the site: each ?demo= scene of the browser build, shot with headless Chrome at 2x.
# Needs `python3 -m http.server 4310` running in static/.
set -e
cd "$(dirname "$0")/.."
CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
PROFILE="${TMPDIR:-/tmp}/margin-shots-profile"
for scene in review brief panel rehearse thesis diagram export library settings; do
  rm -rf "$PROFILE"
  "$CHROME" --headless=new --disable-gpu --hide-scrollbars --no-first-run --user-data-dir="$PROFILE" \
    --force-device-scale-factor=2 --window-size=1440,900 --virtual-time-budget=25000 \
    --screenshot="docs/shots/$scene.png" "http://localhost:4310/?demo=$scene" >/dev/null 2>&1
  printf '%s: %s\n' "$scene" "$(du -h "docs/shots/$scene.png" | cut -f1)"
done
