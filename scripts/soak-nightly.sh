#!/bin/bash
# The nightly soak (docs/dev/soak.md): `tests/run.sh soak 60` on this checkout as it is, its files in
# ~/Library/Logs/Cue/soak/<date-time>/ (report.txt, the two runs' JSON and logs), `latest` pointing at the
# newest, one line per night in history.log. Kept 30 days. Run by launchd (scripts/launchd/) once you've
# installed it, or by hand. Like every test run, it never touches your Cue, its folder or your tmux.
#   scripts/soak-nightly.sh [minutes]      (60 unless told)
set -uo pipefail
unset TMUX TMUX_PANE
REPO="$(cd "$(dirname "$0")/.." && pwd)"
LOGS="$HOME/Library/Logs/Cue/soak"
DIR="$LOGS/$(date +%Y-%m-%d-%H%M)"
mkdir -p "$DIR"
# Awake for the run: a Mac that idles to sleep mid-soak would read as a stall.
CUE_SOAK_DIR="$DIR" caffeinate -i "$REPO/tests/run.sh" soak "${1:-60}" >"$DIR/run.log" 2>&1
status=$?
ln -sfn "$DIR" "$LOGS/latest"
verdict=$([ $status -eq 0 ] && echo PASS || echo FAIL)
echo "$(date '+%Y-%m-%d %H:%M') $verdict $(git -C "$REPO" rev-parse --short HEAD) $DIR/report.txt" >>"$LOGS/history.log"
find "$LOGS" -mindepth 1 -maxdepth 1 -type d -name '20*' -mtime +30 -exec rm -rf {} +
exit $status
