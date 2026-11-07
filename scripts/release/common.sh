# Shared by scripts/release.sh and its parts (sourced, never run): how a release talks, and where it publishes.
REPO="spacegrowth/cue"
say() { printf '  %s\n' "$*"; }
die() { printf 'release: %s\n' "$*" >&2; exit 1; }
# The update files (latest.json, latest-beta.json, update.json): written and read by channel.py.
channel_py() { python3 "$ROOT/scripts/release/channel.py" "$@"; }
