#!/bin/sh
# A remote executor that Cue also holds as a session (its hooks reached Cue from the machine), with its own
# tmux session there: opened from any of its rows, its chat opens AND its own terminal (term_open for it,
# which attaches to that tmux session). Runs tests/preview.html?crew=1&remote=1&held=1&via=… in headless
# Chrome, once per way in: its row in its lead's tree, its chip on the lead chat's team bar, its row in the
# "N live" list. And its lead's own row (in tmux on the same machine, not an executor) still opens only its
# chat: no terminal. Exit 0 when all pass.
set -u
cd "$(dirname "$0")/.."
CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
[ -x "$CHROME" ] || { echo "no Chrome at $CHROME (set CHROME=…)"; exit 2; }
PORT="${PORT:-$((20000 + $$ % 20000))}"
TMP=$(mktemp -d)
python3 -m http.server "$PORT" --bind 127.0.0.1 >/dev/null 2>&1 &
SERVER=$!
trap 'kill $SERVER 2>/dev/null; wait $SERVER 2>/dev/null; rm -rf "$TMP"' EXIT
# Up once it answers.
i=0; until curl -fs "http://127.0.0.1:$PORT/tests/preview.html" >/dev/null 2>&1; do i=$((i + 1)); [ $i -gt 50 ] && { echo "server didn't start"; exit 2; }; perl -e 'select(undef,undef,undef,0.1)'; done
fail=0
# way in : the session that should be open : term_open calls wanted for it
for t in tree:rx-dns:1 chip:rx-dns:1 live:rx-dns:1 lead:rlead:0; do
  via=${t%%:*}; rest=${t#*:}; want=${rest%%:*}; n=${rest#*:}
  dom=$("$CHROME" --headless=new --disable-gpu --no-first-run --user-data-dir="$TMP/profile-$via" --virtual-time-budget=4000 --dump-dom "http://127.0.0.1:$PORT/tests/preview.html?crew=1&remote=1&held=1&still=1&via=$via" 2>/dev/null)
  body=$(printf '%s' "$dom" | grep -o '<body[^>]*>' | head -1 | sed 's/&quot;/"/g')
  opens=$(printf '%s' "$body" | grep -o "\"$want\":[0-9]*" | cut -d: -f2)
  open=$(printf '%s' "$body" | sed -n 's/.*data-open="\([^"]*\)".*/\1/p')
  if [ "${opens:-0}" = "$n" ] && [ "$open" = "$want" ]; then echo "ok   via=$via: $want open, term_open($want)=$n"
  else echo "FAIL via=$via: open=${open:-none} term_open($want)=${opens:-0} (want $want and $n)"; fail=1; fi
done
exit $fail
