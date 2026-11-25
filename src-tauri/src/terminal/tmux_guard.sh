#!/bin/sh
# Cue's tmux guard. Written by Cue on every launch; Claude Code sessions get its folder first on their PATH.
# An agent's tmux command that would end the tmux server your own sessions run on (kill-server, or
# kill-session -a) is refused. Everything else runs the real tmux, unchanged.
# CUE_TMUX_GUARD=off lets one through, for when you asked for it.

here=$(cd "$(dirname "$0")" && pwd -P)

# The real tmux: the first on PATH that isn't this one, else where Homebrew and the system keep it.
real_tmux() {
  old_ifs=$IFS
  IFS=:
  for d in $PATH; do
    if [ -n "$d" ] && [ -x "$d/tmux" ] && [ "$(cd "$d" 2>/dev/null && pwd -P)" != "$here" ]; then
      IFS=$old_ifs
      printf '%s\n' "$d/tmux"
      return 0
    fi
  done
  IFS=$old_ifs
  for c in /opt/homebrew/bin/tmux /usr/local/bin/tmux /usr/bin/tmux; do
    [ -x "$c" ] && { printf '%s\n' "$c"; return 0; }
  done
  return 1
}

real=$(real_tmux) || { echo "tmux: command not found" >&2; exit 127; }
[ "$CUE_TMUX_GUARD" = off ] && exec "$real" "$@"

# What the command would end, and on which server (-S's socket, -L's name; else $TMUX's, else the default).
uid=$(id -u)
base=${TMUX_TMPDIR:-/tmp}
sock=""
ending=""
in_command=""
in_kill_session=""
takes=""
for a in "$@"; do
  if [ -n "$takes" ]; then
    case $takes in
      S) sock=$a ;;
      L) sock="$base/tmux-$uid/$a" ;;
    esac
    takes=""
    continue
  fi
  if [ -z "$in_command" ]; then
    case $a in
      -S) takes=S; continue ;;
      -S*) sock=${a#-S}; continue ;;
      -L) takes=L; continue ;;
      -L*) sock="$base/tmux-$uid/${a#-L}"; continue ;;
      -c|-f|-T) takes=other; continue ;;
      -*) continue ;;
    esac
    in_command=1
  fi
  case $a in
    kill-ser|kill-serv|kill-serve|kill-server) ending="kill-server" ;;
    kill-ses|kill-sess|kill-sessi|kill-sessio|kill-session) in_kill_session=1 ;;
    ";") in_kill_session="" ;;
    -*a*) [ -n "$in_kill_session" ] && ending="kill-session -a" ;;
  esac
done
[ -z "$ending" ] && exec "$real" "$@"

if [ -z "$sock" ]; then
  if [ -n "$TMUX" ]; then sock=${TMUX%%,*}; else sock="$base/tmux-$uid/default"; fi
fi

# A socket's path with its folder's links resolved (/tmp is /private/tmp); fails when the folder isn't there.
resolved() {
  d=$(cd "$(dirname "$1")" 2>/dev/null && pwd -P) && printf '%s/%s\n' "$d" "$(basename "$1")"
}

# No folder, no server there to end.
target=$(resolved "$sock") || exec "$real" "$@"
for p in "/tmp/tmux-$uid/default" "$CUE_TMUX_PROTECT"; do
  [ -n "$p" ] || continue
  q=$(resolved "$p") || continue
  if [ "$q" = "$target" ]; then
    cat >&2 <<EOF
tmux guard (Cue): refused "$ending" on $target.
That is the tmux server the user's own sessions run on: ending it closes every one of them.
Run tmux on a server of your own instead (tmux -S "\$(mktemp -d)/sock" ...) and end that one.
If the user explicitly asked you to end this server, run the command with CUE_TMUX_GUARD=off.
EOF
    exit 1
  fi
done
exec "$real" "$@"
