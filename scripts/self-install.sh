#!/bin/bash
# Build Cue at one commit and install it in place of the running one: what Cue runs when a Desk item is done
# (src-tauri/src/self_install.rs starts it, detached, so it carries on while Cue quits).
#
#   scripts/self-install.sh <repo> <commit> <tag>
#
# <repo> is the Cue checkout the commit is in; <tag> names the work, for status.json. Everything it does is said in
# $CUE_HOME/install/: status.json at every step ({tag, commit, step, log, at}), log (this run's whole output), lock
# (the PID of the one running; one at a time).
#
# It never builds from <repo>'s folder, where other sessions have edits under way: a clean copy at <commit> is kept
# in install/tree (a git worktree of <repo>, reused), with the add-on and node_modules linked in from <repo> and its
# cue-host builds copied, built into <repo>'s target folder so builds stay incremental. The app it replaces is kept
# beside it (Cue.prev.app) and put back, and started again, when the new one doesn't say it's running this commit
# (install/running.json) within CUE_INSTALL_START_WAIT seconds: "rolled_back".
#
# "failed" says why in its log. A build that fails leaves the running Cue alone. Once Cue was quit, a failure to swap
# the apps starts the installed one again; a failure while rolling back (the new Cue won't quit, the old app can't be
# put back) leaves things as the log says, and may leave no Cue running.
#
# Everything it touches can be pointed elsewhere (the tests do):
#   CUE_HOME                 Cue's data folder (required)
#   CUE_INSTALL_APP_DIR      where Cue.app is installed            ($HOME/Applications)
#   CUE_INSTALL_TREE         the clean copy it builds in           ($CUE_HOME/install/tree)
#   CUE_INSTALL_TARGET_DIR   the cargo target folder               (<repo>/src-tauri/target)
#   CUE_INSTALL_BUILD        the build, a shell command run in the copy; $CUE_INSTALL_EXT_ARGS holds the add-on's
#                            switches when it's there              (cue-host, then npx tauri build --bundles app)
#   CUE_INSTALL_BUILT_APP    the app the build makes               ($CUE_INSTALL_TARGET_DIR/release/bundle/macos/Cue.app)
#   CUE_INSTALL_QUIT         quits the running Cue; $CUE_INSTALL_PIDS are the PIDs running the installed app's
#                            program (osascript asks the app to quit; then only those PIDs are stopped)
#   CUE_INSTALL_LAUNCH       starts $CUE_INSTALL_APP               (open, with a clean environment)
#   CUE_INSTALL_SIGN         signs $CUE_INSTALL_APP; ":" for none  (your Developer ID, when this Mac has one)
#   CUE_INSTALL_WAIT         seconds to wait for hosted sessions' turns before quitting (600; 0 for none)
#   CUE_INSTALL_START_WAIT   seconds the new Cue has to say it's running (90)
set -uo pipefail

REPO="${1:?usage: self-install.sh <repo> <commit> <tag>}"
COMMIT="${2:?usage: self-install.sh <repo> <commit> <tag>}"
TAG="${3:-}"
: "${CUE_HOME:?CUE_HOME is the data folder of Cue}"
DIR="$CUE_HOME/install"
APP_DIR="${CUE_INSTALL_APP_DIR:-$HOME/Applications}"
TREE="${CUE_INSTALL_TREE:-$DIR/tree}"
export CARGO_TARGET_DIR="${CUE_INSTALL_TARGET_DIR:-$REPO/src-tauri/target}"
BUILT_APP="${CUE_INSTALL_BUILT_APP:-$CARGO_TARGET_DIR/release/bundle/macos/Cue.app}"
START_WAIT="${CUE_INSTALL_START_WAIT:-90}"
APP="$APP_DIR/Cue.app"
PREV="$APP_DIR/Cue.prev.app"
LOG="$DIR/log"
LOCK="$DIR/lock"
mkdir -p "$DIR"

# Quitting Cue hangs up on what it started: this carries on.
trap '' HUP

now_ms() { echo "$(( $(date +%s) * 1000 ))"; }
say() { printf '%s %s\n' "$(date '+%H:%M:%S')" "$*" >> "$LOG"; }

# A string as a JSON string: control characters dropped, tabs as spaces, lines joined with \n.
json() {
  local s
  s=$(printf '%s' "$1" | tr -d '\000-\010\013-\037' | tr '\t' ' ' | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' | awk 'NR > 1 { printf "%s", "\\n" } { printf "%s", $0 }')
  printf '"%s"' "$s"
}

# status.json: this step, and the log's last 40 lines when $2 is "log". Written whole, then moved into place.
status() {
  local log=""
  [ "${2:-}" = log ] && log="$(tail -n 40 "$LOG" 2>/dev/null)"
  printf '{"tag":%s,"commit":%s,"step":%s,"log":%s,"at":%s}\n' "$(json "$TAG")" "$(json "$COMMIT")" "$(json "$1")" "$(json "$log")" "$(now_ms)" > "$DIR/status.json.tmp"
  mv -f "$DIR/status.json.tmp" "$DIR/status.json"
  say "step: $1"
}

alive() { [ -n "${1:-}" ] && kill -0 "$1" 2>/dev/null; }

# One at a time: the lock holds the PID of the one running. Cue writes this one's PID there as it starts it, so the
# PID found may be this one's own; any other that's gone is a stale lock, free to take.
take_lock() {
  local held
  held="$(cat "$LOCK" 2>/dev/null || true)"
  if [ -n "$held" ] && [ "$held" != "$$" ] && alive "$held"; then
    echo "self-install: another one is running (PID $held)" >&2
    exit 3
  fi
  echo "$$" > "$LOCK"
}
take_lock
trap '[ "$(cat "$LOCK" 2>/dev/null)" = "$$" ] && rm -f "$LOCK"' EXIT

: > "$LOG"
say "self-install of $COMMIT ($TAG) from $REPO"

# running.json's fields, read without anything but the shell: "commit", "pid" or "at".
running_field() {
  sed -n "s/.*\"$1\": *\"\{0,1\}\([^\",}]*\).*/\1/p" "$DIR/running.json" 2>/dev/null | head -1
}

fail() { say "$1"; status failed log; exit 1; }

# ---- building --------------------------------------------------------------------------------------------------
status building

# The clean copy at the commit: made once, then moved to each new commit. Only files git doesn't ignore are cleaned
# away, so the builds kept in it (the helpers build.rs compiles, cue-host's) stay. Reused only when it is a checkout of
# its own: a folder inside some other repo would otherwise have that repo checked out and cleaned.
own_tree() {
  [ -d "$TREE" ] || return 1
  local top; top="$(git -C "$TREE" rev-parse --show-toplevel 2>/dev/null)" || return 1
  [ -n "$top" ] && [ "$(cd "$top" && pwd -P)" = "$(cd "$TREE" && pwd -P)" ]
}
prepare_tree() {
  if own_tree; then
    git -C "$TREE" checkout -q --detach --force "$COMMIT" && git -C "$TREE" clean -fdq
  else
    rm -rf "$TREE"
    git -C "$REPO" worktree prune
    mkdir -p "$(dirname "$TREE")"
    git -C "$REPO" worktree add -q --detach --force "$TREE" "$COMMIT"
  fi
}
prepare_tree >> "$LOG" 2>&1 || fail "couldn't check out $COMMIT in $TREE"
[ "$(git -C "$TREE" rev-parse HEAD 2>/dev/null)" = "$(git -C "$REPO" rev-parse "$COMMIT^{commit}" 2>/dev/null)" ] || fail "$TREE isn't at $COMMIT"

# The add-on (git-ignored, kept outside the repo): linked to wherever the checkout's points.
mkdir -p "$TREE/src-tauri"
rm -f "$TREE/src-tauri/ext"
EXT_ARGS=""
if [ -e "$REPO/src-tauri/ext" ]; then
  ln -s "$(cd "$REPO/src-tauri/ext" && pwd -P)" "$TREE/src-tauri/ext" || fail "couldn't link the add-on into $TREE"
  EXT_ARGS="--features ext"
  [ -e "$TREE/src-tauri/ext/tauri.conf.json" ] && EXT_ARGS="$EXT_ARGS --config src-tauri/ext/tauri.conf.json"
fi
export CUE_INSTALL_EXT_ARGS="$EXT_ARGS"
rm -f "$TREE/node_modules"
[ -e "$REPO/node_modules" ] && ln -s "$REPO/node_modules" "$TREE/node_modules"
mkdir -p "$TREE/src-tauri/binaries" "$TREE/src-tauri/host-builds"
for f in "$REPO"/src-tauri/binaries/cue-host-*; do [ -e "$f" ] && cp -p "$f" "$TREE/src-tauri/binaries/"; done
# cue-host for other machines, as the checkout last built them: build-host.sh then only builds what changed.
if [ -f "$REPO/src-tauri/host-builds/BUILD" ] && ! cmp -s "$REPO/src-tauri/host-builds/BUILD" "$TREE/src-tauri/host-builds/BUILD"; then
  cp -Rp "$REPO/src-tauri/host-builds/." "$TREE/src-tauri/host-builds/"
fi

# An app started from Finder has a short PATH: the build gets the login shell's (cargo, npm, swiftc).
default_build() {
  local login; login="$("${SHELL:-/bin/zsh}" -lc 'printf %s "$PATH"' 2>/dev/null </dev/null || true)"
  export PATH="${login:+$login:}$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
  scripts/build-host.sh || return 1
  # Its error about update signatures at the end (no release signing key) comes after the app is made: the app is
  # what's judged.
  npx tauri build --bundles app $CUE_INSTALL_EXT_ARGS || true
}

say "building in $TREE (target $CARGO_TARGET_DIR) ${EXT_ARGS:+with the add-on}"
rm -rf "$BUILT_APP"
if [ -n "${CUE_INSTALL_BUILD:-}" ]; then
  (cd "$TREE" && eval "$CUE_INSTALL_BUILD") >> "$LOG" 2>&1
else
  (cd "$TREE" && default_build) >> "$LOG" 2>&1
fi
[ -x "$BUILT_APP/Contents/MacOS/cue" ] || fail "the build didn't make $BUILT_APP"

# ---- installing ------------------------------------------------------------------------------------------------
status installing

# The hosted sessions mid-turn (asked over Cue's socket), as install.sh waits for them: quitting Cue ends them.
# Not when cue-host runs them: they carry on while Cue quits.
hosted_turns() {
  /usr/bin/python3 -I - "$CUE_HOME/cue.sock" <<'PY' 2>/dev/null || true
import json, socket, sys
s = socket.socket(socket.AF_UNIX); s.settimeout(3); s.connect(sys.argv[1]); s.sendall(b'{"type":"list"}\n')
buf = b""
while b"\n" not in buf:
    chunk = s.recv(65536)
    if not chunk: break
    buf += chunk
for x in json.loads(buf.split(b"\n")[0]).get("sessions", []):
    if x.get("hosted") and x.get("agent_pid") and x.get("state") == "working":
        print(x["session_id"])
PY
}
wait_for_hosted_turns() {
  local limit="${CUE_INSTALL_WAIT:-600}" waited=0
  for s in "$CUE_HOME"/host/b-*/host.sock; do [ -S "$s" ] && return 0; done
  while [ "$waited" -lt "$limit" ]; do
    [ -z "$(hosted_turns)" ] && return 0
    [ "$waited" -eq 0 ] && say "waiting for hosted sessions' turns to end"
    sleep 3; waited=$((waited + 3))
  done
  say "still mid-turn after ${limit}s: installing anyway"
}

# The installed app's program, as a process shows it (doubled slashes made one).
EXE="$(printf '%s' "$APP/Contents/MacOS/cue" | tr -s /)"

# Whether PID $1 is the installed Cue itself: its program is the app's, run with no arguments. A PID read from a file
# may since be some other process's, and the same program run with arguments is a hook an agent may be waiting on:
# neither is ever stopped.
is_cue() {
  alive "$1" || return 1
  [ "$(ps -o comm= -p "$1" 2>/dev/null | tr -s /)" = "$EXE" ] && [ "$(ps -o args= -p "$1" 2>/dev/null | tr -s /)" = "$EXE" ]
}

# The PIDs running the installed app, whether or not running.json names them (a first install, an older Cue, one
# that crashed). pgrep only finds the candidates; each is checked by is_cue.
app_pids() {
  local p
  for p in $(pgrep -f "$EXE" 2>/dev/null); do is_cue "$p" && echo "$p"; done
  return 0
}

# Of these PIDs, the ones still running the installed Cue.
still_cue() {
  local p
  for p in "$@"; do is_cue "$p" && echo "$p"; done
  return 0
}

# Wait up to $1 tenths of a second for these PIDs ($2…) to be gone (or no longer Cue).
gone() {
  local n=$1; shift
  for _ in $(seq 1 "$n"); do [ -z "$(still_cue "$@")" ] && return 0; sleep 0.1; done
  return 1
}

# Quit the installed Cue and wait until nothing runs its program: asked first, then only those exact PIDs are stopped.
# Fails when one is still there.
quit_cue() {
  local pids; pids="$(app_pids | tr '\n' ' ')"
  [ -n "${pids// /}" ] || return 0
  say "quitting Cue (PID $pids)"
  export CUE_INSTALL_PIDS="$pids"
  if [ -n "${CUE_INSTALL_QUIT:-}" ]; then
    eval "$CUE_INSTALL_QUIT" >> "$LOG" 2>&1 || true
  else
    osascript -e "quit app \"$APP\"" >> "$LOG" 2>&1 || true
  fi
  gone 300 $pids && return 0
  say "Cue didn't quit in 30 s: stopping PID $(still_cue $pids | tr '\n' ' ')"
  kill $(still_cue $pids) 2>/dev/null || true
  gone 50 $pids && return 0
  kill -9 $(still_cue $pids) 2>/dev/null || true
  gone 20 $pids && [ -z "$(app_pids)" ]
}

launch() {
  export CUE_INSTALL_APP="$APP"
  if [ -n "${CUE_INSTALL_LAUNCH:-}" ]; then
    eval "$CUE_INSTALL_LAUNCH" >> "$LOG" 2>&1 || true
  else
    # A clean environment, as the Dock starts it (install.sh says why); retried, as Launch Services can refuse the
    # first open right after a replace.
    for _ in 1 2 3; do
      env -i HOME="$HOME" USER="${USER:-}" LOGNAME="${LOGNAME:-${USER:-}}" SHELL="${SHELL:-/bin/zsh}" TMPDIR="${TMPDIR:-/tmp}" LANG="${LANG:-en_US.UTF-8}" PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
        open "$APP" >> "$LOG" 2>&1 && return 0
      sleep 1
    done
  fi
}

sign() {
  export CUE_INSTALL_APP="$APP"
  if [ -n "${CUE_INSTALL_SIGN:-}" ]; then
    eval "$CUE_INSTALL_SIGN" >> "$LOG" 2>&1 || true
    return
  fi
  local id
  id="$(security find-identity -v -p codesigning 2>/dev/null | sed -n 's/.*"\(Developer ID Application: [^"]*\)".*/\1/p' | head -1)"
  [ -n "$id" ] || return 0
  codesign --force --deep --options runtime --entitlements "$TREE/src-tauri/entitlements.plist" --sign "$id" "$APP" >> "$LOG" 2>&1 || say "not signed: codesign failed"
}

# Cue was quit and the installed app is as it was: start it again, then fail. Never left with no Cue running.
fail_relaunched() { launch; fail "$1"; }

wait_for_hosted_turns
quit_cue || fail_relaunched "Cue didn't quit: the installed app is left as it was"

mkdir -p "$APP_DIR" || fail_relaunched "couldn't make $APP_DIR"
rm -rf "$PREV" 2>/dev/null
[ -e "$PREV" ] && fail_relaunched "couldn't clear $PREV, where the installed app is kept"
HAD_PREV=0
if [ -e "$APP" ]; then
  mv "$APP" "$PREV" || fail_relaunched "couldn't keep the installed app as $PREV"
  HAD_PREV=1
fi
if ! cp -R "$BUILT_APP" "$APP"; then
  rm -rf "$APP"
  [ "$HAD_PREV" = 1 ] && mv "$PREV" "$APP"
  fail_relaunched "couldn't copy the new app into $APP_DIR"
fi
sign

# ---- starting --------------------------------------------------------------------------------------------------
status starting
STARTED_MS="$(now_ms)"
launch

# Whether running.json says a live Cue started since the launch is running this commit; NEW_PID is its PID.
NEW_PID=""
started() {
  local pid at
  pid="$(running_field pid)"; at="$(running_field at)"
  alive "$pid" && [ "${at:-0}" -ge "$((STARTED_MS - 1000))" ] && NEW_PID="$pid" || NEW_PID=""
  [ -n "$NEW_PID" ] && [ "$(running_field commit)" = "$(git -C "$TREE" rev-parse HEAD)" ]
}
for _ in $(seq 1 "$START_WAIT"); do
  started && { say "Cue (PID $NEW_PID) is running $COMMIT"; status installed; exit 0; }
  sleep 1
done
started || true

# ---- rolled back -----------------------------------------------------------------------------------------------
say "the new Cue didn't say it was running $COMMIT in ${START_WAIT}s${NEW_PID:+ (PID $NEW_PID runs $(running_field commit))}: putting the old one back"
# The new one may be running without having said so: whatever runs the installed app's program is quit first.
# One still running from the app isn't swapped under: the new app stays, and that's a failure.
quit_cue || fail "the new Cue didn't quit: it is left installed, and the old app is kept as $PREV"
if [ "$HAD_PREV" = 1 ]; then
  rm -rf "$APP" 2>/dev/null
  [ -e "$APP" ] && fail "couldn't remove the new app to put the old one back (it is kept as $PREV)"
  mv "$PREV" "$APP" || fail "couldn't put the old app back from $PREV: no Cue is installed"
  launch
else
  say "there was no app before this one to put back"
fi
status rolled_back log
exit 1
