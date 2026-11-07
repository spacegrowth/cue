//! The scripts Cue puts on a machine: the hook, the status line and the watcher, with this Mac's name
//! for the watcher's file.

use serde_json::Value;

/// The hook on the machine. Its name has "cue-hook" in it, so Connect's merge replaces older entries.
pub const SCRIPT: &str = r#"#!/bin/sh
# Cue's hook (installed by Cue on your Mac when you added this machine): hands the agent's hook event to
# Cue over your SSH connection. Never in the agent's way: without Cue watching, it leaves at once.
d="$HOME/.cue"
mkdir -p "$d/in" "$d/out" 2>/dev/null || exit 0
id="$(date +%s)-$$"
# A line per run (kept short), for when an event seems to go missing.
[ "$(wc -c 2>/dev/null <"$d/hook.log" || echo 0)" -lt 65536 ] || mv -f "$d/hook.log" "$d/hook.log.old" 2>/dev/null
echo "$id $1 ${TMUX_PANE:-}" >>"$d/hook.log" 2>/dev/null
# The event (a prompt, a command about to run) is yours alone: the file is readable by you only.
input=$(cat)
( umask 077; {
  printf '{"event":"%s","harness":"%s","pane":"%s","tmux":"%s","input":' "$1" "$2" "${TMUX_PANE:-}" "${TMUX:+1}"
  printf '%s' "$input"
  printf '}\n'
} >"$d/in/$id.tmp" ) 2>/dev/null && mv "$d/in/$id.tmp" "$d/in/$id.json"
# A message sent, or a reply ending, in a session with the status list on (~/.cue/status-on, kept by Cue): Cue's
# answer (its list carried forward, or the reply sent back for one), waited for 3 seconds at most while Cue is
# watching. Any other session never waits.
if [ "$1" = prompt ] || [ "$1" = stop ]; then
  sid=$(printf '%s' "$input" | tr -d '\n' | sed -n 's/.*"session_id" *: *"\([A-Za-z0-9-]*\)".*/\1/p')
  [ -n "$sid" ] && grep -qxF "$sid" "$d/status-on" 2>/dev/null || exit 0
  seen=$(cat "$d/watching" 2>/dev/null) || seen=0
  [ $(( $(date +%s) - ${seen:-0} )) -lt 5 ] || exit 0
  end=$(( $(date +%s) + 3 ))
  while [ "$(date +%s)" -le "$end" ]; do
    if [ -e "$d/out/$id.json" ]; then cat "$d/out/$id.json"; rm -f "$d/out/$id.json"; exit 0; fi
    sleep 0.1 2>/dev/null || sleep 1
  done
  exit 0
fi
[ "$1" = permission ] || exit 0
# A permission question: wait for the answer while Cue is watching (it stamps the time every second).
# Not watching now: leave at once. Once it has the question, a dropped line (the Mac asleep, Wi-Fi)
# isn't the end of it: Cue reconnects and the answer still comes, so it waits up to 10 minutes of
# silence. The terminal asks meanwhile either way, so answering there still works.
quiet=20
while :; do
  if [ -e "$d/out/$id.json" ]; then cat "$d/out/$id.json"; rm -f "$d/out/$id.json"; exit 0; fi
  [ -e "$d/in/$id.json" ] || quiet=600
  # The stamp read as the watcher rewrites it comes back empty (once in a thousand reads: enough to hit
  # within minutes at five a second): a moment's thing, not Cue gone, so it counts as now. No stamp at
  # all: Cue never watched.
  seen=$(cat "$d/watching" 2>/dev/null) || seen=0
  [ -n "$seen" ] || seen=$(date +%s)
  [ $(( $(date +%s) - seen )) -lt $quiet ] || { rm -f "$d/in/$id.json"; exit 0; }
  sleep 0.2 2>/dev/null || sleep 1
done
"#;

/// The status line on the machine, as on this Mac (install.sh's cue-statusline): Claude Code's transcript
/// doesn't say how big a session's context window is, its status line does. This passes the size on to
/// Cue through the hook (as a "usage" event when it changes, and again every 10 minutes in case one was
/// lost while Cue couldn't hear the machine; not on every refresh), then runs your
/// own status line command unchanged (saved in ~/.cue/statusline.orig; none there: it shows nothing).
pub const STATUS: &str = r#"#!/bin/sh
# Cue's pass-through status line (installed by Cue on your Mac when you added this machine): tells Cue
# the session's context window when it changes (and every 10 minutes, should one have been lost), then
# runs your own status line command unchanged.
input=$(cat)
d="$HOME/.cue"
one=$(printf '%s' "$input" | tr -d '\n')
sid=$(printf '%s' "$one" | sed -n 's/.*"session_id" *: *"\([A-Za-z0-9-]*\)".*/\1/p')
win=$(printf '%s' "$one" | sed -n 's/.*"context_window_size" *: *\([0-9][0-9]*\).*/\1/p')
if [ -n "$sid" ] && [ -n "$win" ] && { [ "$(cat "$d/win/$sid" 2>/dev/null)" != "$win" ] || [ -z "$(find "$d/win/$sid" -mmin -10 2>/dev/null)" ]; }; then
  mkdir -p "$d/win" 2>/dev/null && printf '%s' "$win" >"$d/win/$sid" 2>/dev/null
  find "$d/win" -type f -mtime +7 -delete 2>/dev/null
  [ -x "$d/cue-hook" ] && printf '%s' "$input" | "$d/cue-hook" usage claude >/dev/null 2>&1
fi
cmd=$(cat "$d/statusline.orig" 2>/dev/null)
[ -n "$cmd" ] && printf '%s' "$input" | sh -c "$cmd"
exit 0
"#;

/// The status line command Cue puts in the machine's Claude Code settings.
pub(super) const STATUS_CMD: &str = "\"$HOME/.cue/cue-statusline\"";

pub(super) fn wrap_status_line(cfg: &Value) -> Option<(Value, String)> {
    crate::config::wrap_status_line(cfg, STATUS_CMD)
}

/// What the watcher runs on the machine: print each waiting event as one line, looking ten times a second
/// (an event reaches Cue in about a tenth of a second, as a hook on this Mac does), and once a second stamp
/// the time and say it's alive. The heartbeat matters: if the Cue that started this quits, the next one
/// fails to reach it, so ssh ends instead of living on and taking events nobody reads. A `sleep` that
/// can't do fractions (not GNU's or BusyBox's) falls back to a second. ("cue-watch" marks these processes.)
/// Once a minute it also drops answers in out/ a day old or more: one the hook never collected (its agent
/// was gone by the time you answered) would otherwise stay there for good.
///
/// One watcher per Mac: some SSH servers never end a command whose connection is gone, so an
/// earlier connection's watcher can live on, its output going nowhere, and take events the current one
/// never sees (prompts and Stops lost, some read by both as an empty line). A watcher records itself in
/// ~/.cue/watcher-<this Mac>, and an older one of the same Mac leaves as soon as that names another
/// (another Mac's watcher is left alone). Once a second it ends watchers of the script before this one,
/// which don't know that file: shells only, known by their script read from /proc, never one carrying
/// this script's marker (itself, and whatever started it with this text in its command line). Each
/// event is claimed (renamed) before it's read, so two never take one, and at start a claimed event
/// whose watcher died is put back. On a machine without /proc or pgrep the sweep does nothing.
pub(super) const WATCH: &str = r#"# cue-watch
d="$HOME/.cue"; mkdir -p "$d/in" "$d/out"
v2=cue-watch-v2; w="$d/watcher-@MAC@"
echo $$ >"$w"
for g in "$d"/in/*.json.*; do
  [ -e "$g" ] || continue
  kill -0 "${g##*.}" 2>/dev/null || mv "$g" "${g%.*}" 2>/dev/null
done
n=0; p=0
while :; do
  [ "$(cat "$w" 2>/dev/null)" = $$ ] || exit 0
  if [ "$n" -eq 0 ]; then
    # Written whole (a rename), never truncated first: a hook reads it between writes.
    date +%s >"$d/watching.new" 2>/dev/null && mv -f "$d/watching.new" "$d/watching"
    for x in $(pgrep -f '# cue-watch' 2>/dev/null); do
      case "$(cat "/proc/$x/comm" 2>/dev/null)" in sh|bash|dash|ash|zsh|ksh|busybox) ;; *) continue ;; esac
      grep -q "$v2" "/proc/$x/cmdline" 2>/dev/null && continue
      grep -q 'n=0; p=0' "/proc/$x/cmdline" 2>/dev/null && kill "$x" 2>/dev/null
    done
  fi
  for f in "$d"/in/*.json; do
    [ -e "$f" ] || continue
    g="$f.$$"; mv "$f" "$g" 2>/dev/null || continue
    printf '@@%s ' "$(basename "$f" .json)"; tr -d '\n' <"$g"; echo; rm -f "$g"
  done
  # The beat comes after the events waiting there: the first one means all of them were handed over.
  if [ "$n" -eq 0 ]; then
    [ "$p" -eq 0 ] && find "$d/out" "$d/in" -name '*.json*' -mmin +1440 -delete 2>/dev/null
    p=$(( (p + 1) % 60 ))
    echo "@@hb" || exit 0
  fi
  n=$(( (n + 1) % 10 ))
  sleep 0.1 2>/dev/null || { sleep 1; n=0; }
done"#;

/// The watcher for this Mac: WATCH with this Mac's name (letters, digits and dashes) in its file's name.
pub(super) fn watch_script() -> String {
    WATCH.replace("@MAC@", &mac_key())
}

fn mac_key() -> String {
    let key: String = hostname().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).take(64).collect();
    if key.is_empty() { "mac".into() } else { key }
}

/// This Mac's host name, as the system has it ("" if it can't say).
pub(crate) fn hostname() -> String {
    let mut buf = [0u8; 256];
    let n = if unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) } == 0 { buf.iter().position(|b| *b == 0).unwrap_or(0) } else { 0 };
    String::from_utf8_lossy(&buf[..n]).into_owned()
}
