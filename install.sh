#!/bin/bash
# Install Cue: the app, the Claude Code hooks, and the Pi extension.
#
#   ./install.sh            everything (app, Claude Code + Codex hooks, Pi extension)
#   ./install.sh --app      just the app
#   ./install.sh --uninstall
#
# Cue's hosted sessions mid-turn are waited for first (quitting Cue ends them): CUE_INSTALL_WAIT=<seconds>, 0 for none.
#
# Safe to re-run. Your ~/.claude/settings.json is backed up before it's touched, and only
# Cue's own hook entries are added or removed — your other hooks stay as they are.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
CUE_HOME="${CUE_HOME:-$HOME/Library/Application Support/dev.spacegrowth.cue}"
OLD_HOME="$HOME/.cue"   # where Cue lived before; kept only for forwarders
SETTINGS="$HOME/.claude/settings.json"
CODEX_HOOKS="$HOME/.codex/hooks.json"
PI_EXT_DIR="$HOME/.pi/agent/extensions"
APP_SRC="$ROOT/src-tauri/target/release/bundle/macos/Cue.app"

say() { printf '  %s\n' "$*"; }

agent_hooks() {  # $1 = file, $2 = claude|codex, $3 = remove (connecting is the app's: cue connect)
  python3 - "$1" "$CUE_HOME/bin/cue-hook" "$2" "$3" <<'EOF'
import json, os, re, shutil, sys, time
path, hook, harness, mode = sys.argv[1:5]
cfg = json.load(open(path)) if os.path.exists(path) else {}
if os.path.exists(path):
    shutil.copy(path, f"{path}.cue-backup-{time.strftime('%Y%m%d-%H%M%S')}")
hooks = cfg.setdefault("hooks", {})
mine = re.compile(r"cue-(claude-)?hook")
# Drop any earlier Cue entries, keep everything else.
for event, groups in list(hooks.items()):
    kept = []
    for g in groups:
        g["hooks"] = [h for h in g.get("hooks", []) if not mine.search(h.get("command", ""))]
        if g["hooks"]:
            kept.append(g)
    hooks[event] = kept
    if not kept:
        del hooks[event]
if not hooks:
    cfg.pop("hooks", None)
os.makedirs(os.path.dirname(path), exist_ok=True)
with open(path, "w") as f:
    json.dump(cfg, f, indent=2)
    f.write("\n")
EOF
}

# Cue's PIDs. "cue$": the app itself. Hooks run the same binary with arguments ("cue hook …") and must
# survive (an agent may be waiting on one for your answer). -a: pgrep leaves out its own ancestors,
# and run from a session Cue started (a hosted one), Cue is one of them.
cue_pids() { pgrep -a -f "Cue.app/Contents/MacOS/cue$" || true; }
# Wait up to $1 tenths of a second for these PIDs ($2…) to be gone.
gone() {
  local n=$1; shift
  for _ in $(seq 1 "$n"); do kill -0 "$@" 2>/dev/null || return 0; sleep 0.1; done
  return 1
}
# The hosted sessions mid-turn, other than the one running this (if it is one): "<id> <name>" a line. Quitting Cue
# ends every session it hosts, and a turn under way is lost, so an install waits for these. Asked over Cue's socket;
# nothing when Cue isn't running.
hosted_turns() {
  /usr/bin/python3 -I - "$CUE_HOME/cue.sock" "${CLAUDE_CODE_SESSION_ID:-}" <<'PY' 2>/dev/null || true
import json, socket, sys
sock, me = sys.argv[1], sys.argv[2]
s = socket.socket(socket.AF_UNIX); s.settimeout(3); s.connect(sock); s.sendall(b'{"type":"list"}\n')
buf = b""
while b"\n" not in buf:
    chunk = s.recv(65536)
    if not chunk: break
    buf += chunk
for x in json.loads(buf.split(b"\n")[0]).get("sessions", []):
    if x.get("hosted") and x.get("agent_pid") and x.get("state") == "working" and x.get("session_id") != me:
        print(x["session_id"], x.get("name") or x.get("project") or "")
PY
}

# Wait for the hosted sessions' turns to end before quitting Cue: up to CUE_INSTALL_WAIT seconds (10 minutes unless
# told), then it goes ahead. CUE_INSTALL_WAIT=0 doesn't wait.
wait_for_hosted_turns() {
  local limit="${CUE_INSTALL_WAIT:-600}" waited=0 busy
  while [ "$waited" -lt "$limit" ]; do
    busy=$(hosted_turns)
    [ -z "$busy" ] && return 0
    [ "$waited" -eq 0 ] && say "waiting for Cue's hosted sessions to finish their turns (quitting Cue would end them):" && echo "$busy" | sed 's/^[^ ]* /    /'
    sleep 3; waited=$((waited + 3))
  done
  say "still mid-turn after ${limit}s: installing anyway (that turn ends; its next message starts it again)"
}

# Ask Cue to quit and wait until it has (opening a new copy mid-shutdown fails with error -600).
quit_cue() {
  local pids; pids=$(cue_pids)
  [ -n "$pids" ] || return 0
  osascript -e 'quit app "Cue"' 2>/dev/null || true
  # Quitting can take a while: Cue closes the Claudes it hosts (up to ~7 s), and one stopped before that
  # leaves them running on their own (Cue ends any it finds on its next start). Past 30 s, these exact
  # PIDs are stopped, and the script waits until they're gone: a dying Cue still counts as running.
  gone 300 $pids && return 0
  kill $pids 2>/dev/null || true
  gone 50 $pids && return 0
  kill -9 $pids 2>/dev/null || true
  gone 20 $pids || true
}

# Start the installed Cue and make sure it is running. Right after a quit-and-replace, Launch
# Services can refuse the first open (error -609, "connection invalid") and the app silently stays
# down, so the open is retried until the process is seen.
launch_cue() {
  # Only a Cue that wasn't running before counts as started.
  local before; before=" $(cue_pids | tr '\n' ' ') "
  for _ in 1 2 3 4 5 6; do
    # A clean environment, as the Dock or Finder starts it: `open` passes on this shell's own, and one
    # inside tmux (an agent's terminal, an action's) would leave Cue thinking it's in tmux too, so every
    # terminal it opens would quit at once ("sessions should be nested with care").
    env -i HOME="$HOME" USER="$USER" LOGNAME="${LOGNAME:-$USER}" SHELL="${SHELL:-/bin/zsh}" TMPDIR="${TMPDIR:-/tmp}" LANG="${LANG:-en_US.UTF-8}" PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
      open "$HOME/Applications/Cue.app" 2>/dev/null || true
    for _ in $(seq 1 20); do
      for p in $(cue_pids); do
        case "$before" in *" $p "*) ;; *) say "Cue is running"; return 0 ;; esac
      done
      sleep 0.25
    done
  done
  echo "  Cue did not start — open ~/Applications/Cue.app yourself (or run: open ~/Applications/Cue.app)" >&2
  return 1
}

install_app() {
  if [ ! -d "$APP_SRC" ]; then
    say "building Cue.app (first time takes a few minutes)…"
    # An optional add-on in src-tauri/ext/ (not part of this repo) is built in when it's there.
    local ext=""; [ -e "$ROOT/src-tauri/ext/mod.rs" ] && ext="--features ext"
    # Its own bundle settings (helper programs it ships beside Cue), when it has them.
    [ -n "$ext" ] && [ -e "$ROOT/src-tauri/ext/tauri.conf.json" ] && ext="$ext --config src-tauri/ext/tauri.conf.json"
    (cd "$ROOT" && npm install --silent && npx tauri build --bundles app $ext >/dev/null)
  fi
  wait_for_hosted_turns
  quit_cue
  mkdir -p "$HOME/Applications"
  rm -rf "$HOME/Applications/Cue.app"
  cp -R "$APP_SRC" "$HOME/Applications/Cue.app"
  say "app       → ~/Applications/Cue.app"
  sign_app "$HOME/Applications/Cue.app"
}

# A build of your own is signed with your Developer ID when this Mac has one (as releases are, without
# notarizing): macOS then knows it's the same app across installs, so the microphone, Accessibility and
# the rest are asked for once, not after every install. Without one it stays as built.
sign_app() {
  local id
  id="$(security find-identity -v -p codesigning 2>/dev/null | sed -n 's/.*"\(Developer ID Application: [^"]*\)".*/\1/p' | head -1)"
  [ -n "$id" ] || return 0
  local err
  if err="$(codesign --force --deep --options runtime --entitlements "$ROOT/src-tauri/entitlements.plist" --sign "$id" "$1" 2>&1)"; then
    say "signed    → with your Developer ID (permissions are kept across installs)"
  else
    say "not signed: codesign failed, so macOS may ask for permissions again: $err"
  fi
}

install_hooks() {
  mkdir -p "$CUE_HOME/bin"
  # The hook is built into Cue's binary. This shim is what the agents' settings run; Cue rewrites
  # it on every launch (so moving the app can't break it), and writing it here too means hooks
  # work before Cue's first launch.
  printf '#!/bin/sh\nexec "%s" hook "$@"\n' "$HOME/Applications/Cue.app/Contents/MacOS/cue" > "$CUE_HOME/bin/cue-hook"
  cp "$ROOT/hooks/cue-ctl" "$CUE_HOME/bin/"   # optional shell tool for developers (needs python3)
  chmod +x "$CUE_HOME/bin/"*
  # Sessions started before Cue moved to Application Support (or before the cue-hook rename)
  # still call ~/.cue/bin/…: forward those to the real hook.
  mkdir -p "$OLD_HOME/bin"
  printf '#!/bin/sh\nexec "%s/bin/cue-hook" "$@"\n' "$CUE_HOME" > "$OLD_HOME/bin/cue-hook"
  printf '#!/bin/sh\nexec "%s/bin/cue-hook" "$1" claude\n' "$CUE_HOME" > "$OLD_HOME/bin/cue-claude-hook"
  printf '#!/bin/sh\nexec "%s/bin/cue-ctl" "$@"\n' "$CUE_HOME" > "$OLD_HOME/bin/cue-ctl"
  chmod +x "$OLD_HOME/bin/"*
  [ -f "$CUE_HOME/config.json" ] || echo '{ "pi": { "gate": "dangerous" } }' > "$CUE_HOME/config.json"
  # The app connects the agents itself (the same as its Settings → Connect).
  "$HOME/Applications/Cue.app/Contents/MacOS/cue" connect claude >/dev/null
  say "claude    → hooks in ~/.claude/settings.json (backup saved next to it)"
  if command -v codex >/dev/null || [ -d "$HOME/.codex" ]; then
    "$HOME/Applications/Cue.app/Contents/MacOS/cue" connect codex >/dev/null
    say "codex     → hooks in ~/.codex/hooks.json — in Codex, run /hooks once to trust them"
  fi
  if command -v pi >/dev/null; then
    mkdir -p "$PI_EXT_DIR"
    ln -sf "$ROOT/pi/cue.ts" "$PI_EXT_DIR/cue.ts"
    say "pi        → ~/.pi/agent/extensions/cue.ts (gate: see config.json in $CUE_HOME)"
  fi
}

# Cue's status line (Cue puts it in front of yours when Claude Code is connected; config.rs): on
# --uninstall, yours goes back (saved in statusline.orig). Left alone if you've changed it since.
statusline_remove() {
  python3 - "$SETTINGS" "$CUE_HOME/statusline.orig" <<'EOF'
import json, os, shutil, sys, time
path, orig = sys.argv[1:]
try:
    cfg = json.load(open(path))
except Exception:
    sys.exit(0)
line = cfg.get("statusLine") or {}
if "cue-statusline" not in line.get("command", ""):
    sys.exit(0)
prev = open(orig).read() if os.path.exists(orig) else ""
if prev:
    cfg["statusLine"] = {**line, "command": prev}
else:
    cfg.pop("statusLine", None)
shutil.copy(path, f"{path}.cue-backup-{time.strftime('%Y%m%d-%H%M%S')}")
with open(path, "w") as f:
    json.dump(cfg, f, indent=2)
    f.write("\n")
EOF
}

uninstall() {
  statusline_remove; say "status line restored"
  agent_hooks "$SETTINGS" claude remove; say "claude hooks removed (backup saved)"
  if [ -f "$CODEX_HOOKS" ]; then agent_hooks "$CODEX_HOOKS" codex remove; say "codex hooks removed (backup saved)"; fi
  rm -f "$PI_EXT_DIR/cue.ts"; say "pi extension removed"
  quit_cue
  rm -rf "$HOME/Applications/Cue.app" "$CUE_HOME/bin" "$OLD_HOME/bin"; say "app removed (your data is kept in $CUE_HOME)"
}

case "${1:-}" in
  --uninstall) uninstall ;;
  --app) install_app; launch_cue ;;
  "") install_app; install_hooks
      launch_cue; say "done — new and running sessions pick up the hooks on their next event" ;;
  *) sed -n '2,9p' "$0"; exit 2 ;;
esac
