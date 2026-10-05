#!/bin/bash
# Run every test against a private, silent Cue: its own socket (CUE_HOME), no notifications,
# no windows (CUE_QUIET). Your real Cue keeps running untouched.
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export CUE_HOME="$(mktemp -d /tmp/cue-test.XXXX)"
export CUE_QUIET=1
export CUE_BIN="$ROOT/src-tauri/target/debug/cue"
(cd "$ROOT/src-tauri" && cargo test -q 2>&1 | grep -E "test result|FAILED|panicked" ; cargo build -q)
"$CUE_BIN" >/dev/null 2>&1 &
trap 'pkill -f "$CUE_BIN"; rm -rf "$CUE_HOME"' EXIT   # every test Cue process, hooks included
for i in $(seq 1 50); do [ -S "$CUE_HOME/cue.sock" ] && break; sleep 0.2; done
python3 "$ROOT/tests/test_protocol.py"
