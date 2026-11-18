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
# Before the build, the window is checked in that copy (scripts/check-window.mjs): a commit whose window scripts call a
# name none of them declares, or declare one twice, would install a window that doesn't open. It is refused, "failed",
# and its log says which names.
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
#   CUE_INSTALL_CHECK        the window's check, a shell command run in the copy before the build; ":" for none
#                            (node scripts/check-window.mjs ., when the commit has it)
#   CUE_INSTALL_BUILD        the build, a shell command run in the copy; $CUE_INSTALL_EXT_ARGS holds the add-on's
#                            switches when it's there              (cue-host, then npx tauri build --bundles app)
#   CUE_INSTALL_BUILT_APP    the app the build makes               ($CUE_INSTALL_TARGET_DIR/release/bundle/macos/Cue.app)
#   CUE_INSTALL_QUIT         quits the running Cue; $CUE_INSTALL_PIDS are the PIDs running the installed app's
#                            program (osascript asks the app to quit; then only those PIDs are stopped)
#   CUE_INSTALL_LAUNCH       starts $CUE_INSTALL_APP               (open, with a clean environment)
#   CUE_INSTALL_SIGN         signs $CUE_INSTALL_APP, the built app, before Cue is quit; ":" for none
#                            (your Developer ID, when this Mac has one)
#
# It never waits for sessions' turns: Cue starts it when it's time (the Desk waits on sessions that end with Cue, unless
# you say Install now), and the sessions cue-host runs carry on while Cue quits.
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

# An app started from Finder has a short PATH: the check and the build get the login shell's (node, cargo, npm, swiftc).
login_path() {
  local login; login="$("${SHELL:-/bin/zsh}" -lc 'printf %s "$PATH"' 2>/dev/null </dev/null || true)"
  export PATH="${login:+$login:}$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
}

# The window checked in the copy: its scripts' names, then the window loaded (scripts/check-window.mjs says how). A
# commit from before the check has none to run.
default_check() {
  [ -f scripts/check-window.mjs ] || { echo "no window check at this commit"; return 0; }
  login_path
  node scripts/check-window.mjs .
}

default_build() {
  login_path
  scripts/build-host.sh || return 1
  # Its error about update signatures at the end (no release signing key) comes after the app is made: the app is
  # what's judged.
  npx tauri build --bundles app $CUE_INSTALL_EXT_ARGS || true
}

say "checking the window in $TREE"
if [ -n "${CUE_INSTALL_CHECK:-}" ]; then
  (cd "$TREE" && eval "$CUE_INSTALL_CHECK") >> "$LOG" 2>&1
else
  (cd "$TREE" && default_check) >> "$LOG" 2>&1
fi || fail "refused: the window at $COMMIT doesn't load (above: which names). Nothing was built or installed."

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
# that crashed). pgrep only finds the candidates; each is checked by is_cue. -a: pgrep leaves out its own ancestors,
# and started by the Desk, Cue is this helper's parent (unquit, it kept running and the new app was never started).
app_pids() {
  local p
  for p in $(pgrep -a -f "$EXE" 2>/dev/null); do is_cue "$p" && echo "$p"; done
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

# The built app signed where it was built, before Cue is quit: signing it once it's in $APP_DIR is refused there to
# anything without App Management ("Operation not permitted" on a helper inside it), and half signed it won't start.
# A signature that fails leaves the running Cue alone.
sign() {
  export CUE_INSTALL_APP="$BUILT_APP"
  if [ -n "${CUE_INSTALL_SIGN:-}" ]; then
    (eval "$CUE_INSTALL_SIGN") >> "$LOG" 2>&1
    return
  fi
  local id
  id="$(security find-identity -v -p codesigning 2>/dev/null | sed -n 's/.*"\(Developer ID Application: [^"]*\)".*/\1/p' | head -1)"
  [ -n "$id" ] || return 0
  codesign --force --deep --options runtime --entitlements "$TREE/src-tauri/entitlements.plist" --sign "$id" "$BUILT_APP" >> "$LOG" 2>&1
}

# Cue was quit and the installed app is as it was: start it again, then fail. Never left with no Cue running.
fail_relaunched() { launch; fail "$1"; }

sign || fail "couldn't sign $BUILT_APP (above: why). The running Cue is left alone."
quit_cue || fail_relaunched "Cue didn't quit: the installed app is left as it was"

mkdir -p "$APP_DIR" || fail_relaunched "couldn't make $APP_DIR"
# A kept app with none installed (an earlier run stopped between its moves) is the only copy: put it back first.
if [ ! -e "$APP" ] && [ -e "$PREV" ]; then
  mv "$PREV" "$APP" || fail "no app is installed and the one kept as $PREV couldn't be put back"
fi
rm -rf "$PREV" 2>/dev/null
[ -e "$PREV" ] && fail_relaunched "couldn't clear $PREV, where the installed app is kept"
HAD_PREV=0
if [ -e "$APP" ]; then
  mv "$APP" "$PREV" || fail_relaunched "couldn't keep the installed app as $PREV"
  HAD_PREV=1
fi
if ! cp -R "$BUILT_APP" "$APP"; then
  rm -rf "$APP"
  if [ "$HAD_PREV" = 1 ] && ! mv "$PREV" "$APP"; then
    fail "couldn't copy the new app into $APP_DIR, nor put the old one back: it is kept as $PREV"
  fi
  fail_relaunched "couldn't copy the new app into $APP_DIR"
fi

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
  # No earlier app to put back: the new one is all there is, so it runs again and this is a failure, not a roll back.
  launch
  fail "the new Cue didn't say it was running $COMMIT, and there was no earlier app to put back: the new one is left installed"
fi
status rolled_back log
exit 1
