# What only a debug build may carry (sourced by scripts/release.sh, never run): the soak's chaos knobs
# (src-tauri/src/chaos, read from CUE_CHAOS_* in debug builds only) and the e2e tests' WebDriver server (the
# `e2e` feature). A release binary that names either was built wrong: it isn't released.
DEBUG_ONLY_TRACES=(CUE_CHAOS_ TAURI_WEBDRIVER_PORT wdio)

# `bin` carries none of DEBUG_ONLY_TRACES, or the release stops here.
check_debug_only() {
  local bin="$1" trace
  for trace in "${DEBUG_ONLY_TRACES[@]}"; do
    [ "$(strings -a "$bin" | grep -c "$trace")" = 0 ] || die "$(basename "$bin") carries debug-only code ($trace): not releasing it"
  done
}
