#!/bin/sh
# Headless regression run of the browser build: scripts/selftest.sh  (needs `python3 -m http.server 4310` in static/)
cd "$(dirname "$0")/.."
CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
PROFILE="${TMPDIR:-/tmp}/margin-selftest-profile"; rm -rf "$PROFILE"
"$CHROME" --headless=new --disable-gpu --no-first-run --user-data-dir="$PROFILE" --window-size=1440,900 --virtual-time-budget=60000 \
  --dump-dom "http://localhost:4310/?selftest=1" 2>/dev/null | python3 -c '
import sys, re, html
dom = sys.stdin.read()
m = re.search(r"<title>(.*?)</title>", dom, re.S)
title = html.unescape(m.group(1)) if m else "no title"
print(title)
box = re.search(r"<pre style=\"position:fixed[^>]*>(.*?)</pre>", dom, re.S)
if box: print(html.unescape(re.sub(r"<[^>]+>", "", box.group(1))))
sys.exit(0 if title.startswith("SELFTEST PASS") else 1)'
