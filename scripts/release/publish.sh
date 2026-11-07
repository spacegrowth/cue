# Publishing to GitHub (sourced by scripts/release.sh; docs/dev/releasing.md). Cue's updater asks
# releases/latest/download/latest.json (stable) or latest-beta.json beside it (beta), so both files live on the
# latest stable release. Every release also carries its own update.json, which a promote copies from.
# latest-beta.json is never older than latest.json: a beta user always gets stable releases too.

# The beta file as published now, into folder $1. Only "no such asset" (before the first beta) leaves it empty:
# any other failure stops the release, since reading it as missing could put an older beta file in its place.
fetch_beta_file() {
  local said
  said="$(gh release download --repo "$REPO" --pattern latest-beta.json --dir "$1" --clobber 2>&1 >/dev/null)" && return 0
  case "$said" in
    *"no assets match the file pattern"*) return 0 ;;
    *) die "couldn't read the beta file on GitHub: $said" ;;
  esac
}

# A stable release: everyone's update, and beta's too unless a newer beta is out.
publish_stable() {
  local beta_now="$WORK/beta-now"
  mkdir -p "$beta_now"
  fetch_beta_file "$beta_now"
  cp "$DIST/update.json" "$DIST/latest.json"
  channel_py newer "$DIST/update.json" "$beta_now/latest-beta.json" >"$DIST/latest-beta.json"
  gh release create "v$VERSION" --repo "$REPO" --title "Cue $VERSION" --notes "$(release_body)" \
    "$DMG" "$DIST/Cue.app.tar.gz" "$DIST/update.json" "$DIST/latest.json" "$DIST/latest-beta.json"
  say "published → https://github.com/$REPO/releases/tag/v$VERSION"
}

# A beta: a prerelease of its own (GitHub's latest stays the stable one), and the beta file pointed at it.
publish_beta() {
  local stable
  stable="$(latest_stable_tag)" || die "no stable release to put the beta file on"
  cp "$DIST/update.json" "$DIST/latest-beta.json"
  gh release create "v$VERSION" --repo "$REPO" --prerelease --title "Cue $VERSION (beta)" --notes "$(release_body)" \
    "$DMG" "$DIST/Cue.app.tar.gz" "$DIST/update.json"
  gh release upload "$stable" --repo "$REPO" --clobber "$DIST/latest-beta.json"
  say "published as a beta → https://github.com/$REPO/releases/tag/v$VERSION (beta file on $stable)"
}

# A beta a day old (or a --hotfix) becomes stable: its own update.json, the same signed build, nothing rebuilt.
promote() {
  local dir age
  # (A global for the trap: it runs after this function's locals are gone.)
  PROMOTE_WORK="$(mktemp -d)"; trap 'rm -rf "$PROMOTE_WORK"' EXIT
  dir="$PROMOTE_WORK"
  [ "$(gh release view "v$VERSION" --repo "$REPO" --json isPrerelease --jq .isPrerelease)" = true ] ||
    die "v$VERSION isn't a beta on GitHub (already stable, or not released)"
  gh release download "v$VERSION" --repo "$REPO" --pattern update.json --dir "$dir" ||
    die "v$VERSION has no update.json: only a beta this script published can be promoted"
  [ "$(channel_py version "$dir/update.json")" = "$VERSION" ] || die "v$VERSION's update.json offers another version"
  age="$(channel_py age-hours "$dir/update.json")"
  if [ "$age" -lt 24 ]; then
    [ -n "$HOTFIX" ] || die "v$VERSION has been on beta ${age} h: give it a day, or --hotfix \"why\""
    say "promoting after ${age} h on beta: hotfix, $HOTFIX"
  fi
  mkdir "$dir/beta-now"
  fetch_beta_file "$dir/beta-now"
  cp "$dir/update.json" "$dir/latest.json"
  channel_py newer "$dir/update.json" "$dir/beta-now/latest-beta.json" >"$dir/latest-beta.json"
  if [ "$DRY_RUN" = 1 ]; then
    say "dry run: v$VERSION (on beta ${age} h) would become stable, the same build; nothing changed on GitHub"
    return 0
  fi
  gh release upload "v$VERSION" --repo "$REPO" --clobber "$dir/latest.json" "$dir/latest-beta.json"
  if [ -n "$HOTFIX" ]; then
    local body
    body="$(gh release view "v$VERSION" --repo "$REPO" --json body --jq .body)"
    gh release edit "v$VERSION" --repo "$REPO" --notes "$(printf '%s\n\nPromoted after %s h on beta, a hotfix: %s' "$body" "$age" "$HOTFIX")"
  fi
  gh release edit "v$VERSION" --repo "$REPO" --prerelease=false --latest --title "Cue $VERSION"
  say "promoted → https://github.com/$REPO/releases/tag/v$VERSION is everyone's update now"
}
