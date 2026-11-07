#!/bin/bash
# Release Cue: the gates first (every test suite, the soak, the lints, the inventory, a clean tree, the Swift
# helpers from scratch), then a build for Apple silicon + Intel, signed with your Developer ID, notarized by
# Apple, a disk image to download, and an update the app's auto-updater accepts. docs/dev/releasing.md has it all.
#
#   scripts/release.sh 0.1.1 "What changed"              gates, build, sign, notarize → dist/v0.1.1/
#   scripts/release.sh 0.1.1 "What changed" --beta       …and publish it as a beta (Settings → Get beta builds)
#   scripts/release.sh --promote 0.1.1                   that beta, a day later, becomes everyone's update
#   scripts/release.sh 0.1.1 "What changed" --publish    …or publish it as stable straight away
#   scripts/release.sh 0.1.1 "What changed" --beta --dry-run    the gates and checks only, then stop
#   --hotfix "<why>" allows a second release in a day; --skip-gates "<why>" goes without the gates. Both reasons
#   are recorded, in dist/v<version>/release-record.txt and in the release's notes on GitHub.
#
# Needs (once, on this Mac):
#   - a "Developer ID Application" certificate in your keychain (Xcode → Settings → Accounts)
#   - the update key at ~/.tauri/cue.key, its password in Keychain as "cue-updater-key"
#   - notarytool credentials saved as a keychain profile, "cue-notary" unless NOTARY_PROFILE says
#     otherwise (an app-specific password from appleid.apple.com):
#       xcrun notarytool store-credentials cue-notary --apple-id <you@example.com> --team-id <TEAM>
#     Without it the build is signed but not notarized (fine to try locally, not to publish).
#     Or, in .release.env, NOTARY_APPLE_ID and NOTARY_TEAM_ID, with the app-specific password in Keychain:
#       security add-generic-password -a <you@example.com> -s cue-notary-password -w
#   - to publish: gh, logged in, with push access to the repo (scripts/release/common.sh)
# Local overrides (e.g. NOTARY_PROFILE=…) can go in .release.env at the repo root (git-ignored).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
for part in common args preflight gates record debug_only build publish; do
  # shellcheck disable=SC1090
  . "$ROOT/scripts/release/$part.sh"
done

# What a run leaves when it stops: no scratch folders, and unless it finished, the version files as committed.
on_exit() {
  local status=$?
  [ -z "${WORK:-}" ] || rm -rf "$WORK"
  [ -z "${PROMOTE_WORK:-}" ] || rm -rf "$PROMOTE_WORK"
  if [ "$status" != 0 ] && [ "$BUMPED" = 1 ]; then restore_version; fi
}
trap on_exit EXIT

parse_args "$@"
# shellcheck disable=SC1091
[ -f .release.env ] && . ./.release.env

if [ "$PROMOTE" = 1 ]; then
  run_preflight
  promote
  exit 0
fi

check_unreleased
run_preflight
# A dry run's logs and record go beside a real build's, never over them.
if [ "$DRY_RUN" = 1 ]; then DIST="$ROOT/dist/dry-run-v$VERSION"; else DIST="$ROOT/dist/v$VERSION"; fi
rm -rf "$DIST"; mkdir -p "$DIST"

[ "$DRY_RUN" = 1 ] || signing_prerequisites   # before the gates, so a missing key doesn't wait out a soak
run_gates "$DIST/gates"
write_record "$DIST/release-record.txt"

if [ "$DRY_RUN" = 1 ]; then
  say "dry run: Cue $VERSION ($(channel_name)) would be built now; nothing built, nothing published"
  sed 's/^/    /' "$DIST/release-record.txt"
  exit 0
fi

build_release
case "$CHANNEL" in
  stable) publish_stable ;;
  beta) publish_beta ;;
  none) say "not published (--beta or --publish to create the GitHub release)" ;;
esac
