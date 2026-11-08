# Build a release (sourced by scripts/release.sh): Apple silicon + Intel, signed with your Developer ID, notarized
# by Apple, its symbols kept, an update the app's updater accepts (update.json) and a disk image, into $DIST.

# What signs it: sets IDENTITY, KEY_PASSWORD, NOTARY_AUTH and NOTARIZE (0: signed only, never published).
signing_prerequisites() {
  local profile="${NOTARY_PROFILE:-cue-notary}" notary_check notary_password
  IDENTITY="$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application: [^"]*\)".*/\1/p' | head -1)"
  [ -n "$IDENTITY" ] || die "no Developer ID Application certificate in your keychain"
  [ -f "$HOME/.tauri/cue.key" ] || die "no update key at ~/.tauri/cue.key"
  KEY_PASSWORD="$(security find-generic-password -s cue-updater-key -w 2>/dev/null)" || die "no 'cue-updater-key' password in Keychain"
  # How notarytool signs in: with NOTARY_APPLE_ID and NOTARY_TEAM_ID set (.release.env), an app-specific password
  # kept in Keychain as "cue-notary-password", read once here like the update key's; else the keychain profile.
  if [ -n "${NOTARY_APPLE_ID:-}" ] && [ -n "${NOTARY_TEAM_ID:-}" ]; then
    notary_password="$(security find-generic-password -s cue-notary-password -w 2>/dev/null)" || die "no 'cue-notary-password' password in Keychain"
    NOTARY_AUTH=(--apple-id "$NOTARY_APPLE_ID" --team-id "$NOTARY_TEAM_ID" --password "$notary_password")
  else
    NOTARY_AUTH=(--keychain-profile "$profile")
  fi
  NOTARIZE=1
  if ! notary_check="$(xcrun notarytool history "${NOTARY_AUTH[@]}" 2>&1)"; then
    printf '%s\n' "$notary_check" | head -5 >&2   # what notarytool said (a missing profile, or Apple unreachable)
    [ "$CHANNEL" = none ] || die "can't publish without notarizing: no working notarytool profile '$profile' (see docs/dev/releasing.md)"
    NOTARIZE=0
    say "no notarytool profile '$profile': signing only, not notarizing"
  fi
}

notarize() {  # $1 = a zip or a dmg; waits for Apple's verdict
  local out
  out="$(xcrun notarytool submit "$1" "${NOTARY_AUTH[@]}" --wait 2>&1)" || true
  grep -q "status: Accepted" <<<"$out" || { printf '%s\n' "$out" >&2; die "Apple didn't accept $(basename "$1") (xcrun notarytool log <id> says why)"; }
}

# The files the version is written in. release-next.sh commits them once the release is published; a run that
# stops before that puts them back as committed (restore_version), so the next try starts from a clean tree.
VERSION_FILES="package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock"
BUMPED=0

# The version, everywhere it's written.
bump_version() {
  BUMPED=1
  python3 - "$VERSION" <<'PY'
import json, re, sys
v = sys.argv[1]
for path in ("src-tauri/tauri.conf.json", "package.json"):
    d = json.load(open(path)); d["version"] = v
    open(path, "w").write(json.dumps(d, indent=2, ensure_ascii=False) + "\n")
p = "src-tauri/Cargo.toml"; s = open(p).read()
open(p, "w").write(re.sub(r'(?m)^version = "[^"]*"', f'version = "{v}"', s, count=1))
PY
  say "version  → $VERSION"
}

# The version files as committed again. Safe: the clean-tree check guaranteed nothing else was in them.
restore_version() {
  # shellcheck disable=SC2086
  git checkout -q HEAD -- $VERSION_FILES && say "the run didn't finish: the version files are back as committed"
}

# Both chip types, signed with the hardened runtime. Sets APP.
build_app() {
  rustup target add x86_64-apple-darwin aarch64-apple-darwin >/dev/null
  export TAURI_SIGNING_PRIVATE_KEY="$HOME/.tauri/cue.key" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$KEY_PASSWORD"
  local config='{"bundle":{"macOS":{"signingIdentity":"'"$IDENTITY"'","hardenedRuntime":true,"entitlements":"entitlements.plist"}}}'
  # Build paths (this Mac's home folder) are written into the app for error messages: rewrite them.
  export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME/.cargo=/cargo --remap-path-prefix=$ROOT=/cue --remap-path-prefix=$HOME=/home"
  say "building (both chip types; a few minutes)…"
  # cue-host for other machines, carried in the app; its macOS program signed as the app is.
  scripts/build-host.sh --check --sign "$IDENTITY"
  npx tauri build --target universal-apple-darwin --bundles app --config "$config"
  APP="src-tauri/target/universal-apple-darwin/release/bundle/macos/Cue.app"
}

# Checks before anything leaves this Mac. Releases never include an add-on (src-tauri/ext): the build doesn't use
# the `ext` feature, and this stops if the app contains any of its code anyway.
check_app() {
  local trace bin
  for trace in axum "cue://pair" jsonwebtoken; do
    [ "$(strings "$APP/Contents/MacOS/cue" | grep -c "$trace")" = 0 ] || die "the app contains add-on code ($trace): not releasing it"
  done
  for bin in "$APP"/Contents/MacOS/*; do
    [ "$(strings "$bin" | grep -c "$HOME")" = 0 ] || die "$(basename "$bin") contains this Mac's home folder path: not releasing it"
  done
  check_debug_only "$APP/Contents/MacOS/cue"
  codesign --verify --deep --strict "$APP" || die "the signature doesn't check out"
  lipo -archs "$APP/Contents/MacOS/cue" | grep -q x86_64 || die "not a universal build"
}

uuids() { dwarfdump --uuid "$1" | awk '{print $2, $3}' | sort; }

# Symbols: Cue.dSYM, so addresses in a hang report from this build resolve (atos). rustc wrote each chip's
# cue.dSYM as it linked (Cargo.toml's release profile), before stripping the binary's debug info. The two become
# one, as the app's binary did, checked to be this very build's (same UUIDs), and stay in dist/ with their UUIDs;
# they're not published.
keep_dsym() {
  local dsym="$WORK/Cue.dSYM" dwarf app_uuids dsym_uuids
  dwarf="$dsym/Contents/Resources/DWARF"
  cp -RL src-tauri/target/aarch64-apple-darwin/release/cue.dSYM "$dsym" || die "no dSYM from the build (see Cargo.toml's release profile)"   # (a link into deps/)
  rm -f "$dwarf"/*
  lipo -create -output "$dwarf/cue" src-tauri/target/{aarch64,x86_64}-apple-darwin/release/cue.dSYM/Contents/Resources/DWARF/* || die "couldn't join the two dSYMs"
  app_uuids="$(uuids "$APP/Contents/MacOS/cue")"; dsym_uuids="$(uuids "$dsym")"
  # (Two empty lists would compare equal: none read is a failure, not a match.)
  [ -n "$app_uuids" ] && [ -n "$dsym_uuids" ] || die "couldn't read the UUIDs of the app or its dSYM (dwarfdump --uuid)"
  [ "$dsym_uuids" = "$app_uuids" ] || die "the dSYM isn't this build's (its UUIDs differ from the app's)"
  ditto -c -k --keepParent "$dsym" "$DIST/Cue.dSYM.zip"
  printf 'Cue %s, dSYM UUIDs (dwarfdump --uuid):\n%s\n' "$VERSION" "$app_uuids" >"$DIST/dSYM-uuids.txt"
}

# Notarize the app and staple Apple's ticket to it.
notarize_app() {
  [ "$NOTARIZE" = 1 ] || return 0
  say "notarizing the app…"
  ditto -c -k --keepParent "$APP" "$WORK/Cue.zip"
  notarize "$WORK/Cue.zip"
  xcrun stapler staple "$APP" >/dev/null || die "couldn't staple the app's notarization"
  spctl --assess --type execute "$APP" || die "Gatekeeper rejects the app"
}

# The update: the (notarized) app as tar.gz, signed with the update key, and update.json, its entry for the
# updater. Its URL is this release's, beta or stable alike, so a promote reuses it as it is.
make_update() {
  local sig
  tar -czf "$DIST/Cue.app.tar.gz" -C "$(dirname "$APP")" Cue.app
  # (Without the build's key variables: the signer refuses a key given twice.)
  # (The password through its variable, never as an argument: those show in `ps` while it runs.)
  env -u TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$KEY_PASSWORD" npx tauri signer sign -f "$HOME/.tauri/cue.key" "$DIST/Cue.app.tar.gz" >/dev/null
  sig="$(cat "$DIST/Cue.app.tar.gz.sig")"; rm "$DIST/Cue.app.tar.gz.sig"
  channel_py entry "$VERSION" "$NOTES" "$sig" "https://github.com/$REPO/releases/download/v$VERSION/Cue.app.tar.gz" "$DIST/update.json"
}

# The download: a disk image (open it, drag Cue to Applications), signed and notarized too. Sets DMG.
make_dmg() {
  local try
  mkdir -p "$WORK/dmg"
  ditto "$APP" "$WORK/dmg/Cue.app"
  ln -s /Applications "$WORK/dmg/Applications"
  DMG="$DIST/Cue-$VERSION.dmg"
  # hdiutil now and then says "Resource busy" (something on the Mac still reading the folder): it's tried again.
  for try in 1 2 3; do
    hdiutil create -volname "Cue" -srcfolder "$WORK/dmg" -ov -format UDZO "$DMG" >/dev/null && break
    [ "$try" = 3 ] && die "couldn't make the disk image"
    sleep 10
  done
  codesign --force --sign "$IDENTITY" --timestamp "$DMG" || die "couldn't sign the disk image"
  if [ "$NOTARIZE" = 1 ]; then
    say "notarizing the disk image…"
    notarize "$DMG"
    xcrun stapler staple "$DMG" >/dev/null || die "couldn't staple the disk image's notarization"
    spctl --assess --type open --context context:primary-signature "$DMG" || die "Gatekeeper rejects the disk image"
    say "signed, notarized and accepted by Gatekeeper"
  else
    say "signed (not notarized)"
  fi
}

build_release() {
  WORK="$(mktemp -d)"   # (removed by release.sh's on_exit)
  bump_version
  build_app
  check_app
  keep_dsym
  notarize_app
  make_update
  make_dmg
  say "ready    → $DIST"
  ls -1 "$DIST" | sed 's/^/             /'
}
