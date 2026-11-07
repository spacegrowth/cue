# What a release leaves behind to read later (sourced by scripts/release.sh): its record in dist/, and its notes
# on GitHub, which say when it went out without its gates or as a hotfix.

channel_name() {
  case "$CHANNEL" in stable) echo "stable" ;; beta) echo "beta" ;; *) echo "not published" ;; esac
}

# The record: which commit, which gates (or why none), why a hotfix, and the toolchain, so a build can at least be
# identified later (true reproducibility isn't on offer with SwiftPM and Xcode in the build).
write_record() {  # $1 = the file
  {
    printf 'Cue %s (%s), %s\n' "$VERSION" "$(channel_name)" "$(date '+%Y-%m-%d %H:%M %Z')"
    # (The build is that commit with the version written in VERSION_FILES; release-next.sh commits that after.)
    printf 'commit: %s, plus the version set to %s\n' "$(git rev-parse HEAD)" "$VERSION"
    if [ -n "$SKIP_GATES" ]; then
      printf 'gates: SKIPPED (all but the clean tree), %s\n' "$SKIP_GATES"
    else
      printf 'gates passed: %s\ngate logs: %s\n' "$GATES_PASSED" "$GATE_LOGS"
    fi
    if [ -n "$HOTFIX" ]; then printf 'hotfix: %s\n' "$HOTFIX"; fi
    printf 'Cargo.lock sha256: %s\n' "$(shasum -a 256 src-tauri/Cargo.lock | cut -d' ' -f1)"
    printf 'rustc: %s\n' "$(rustc --version)"
  } >"$1"
}

# The release's notes on GitHub: what changed, then anything it skipped.
release_body() {
  printf '%s' "$NOTES"
  if [ -n "$SKIP_GATES" ]; then printf '\n\nReleased without its gates: %s' "$SKIP_GATES"; fi
  if [ -n "$HOTFIX" ]; then printf '\n\nHotfix: %s' "$HOTFIX"; fi
}
