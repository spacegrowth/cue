#!/bin/bash
# Build a Cue release: Apple silicon + Intel, signed with your Developer ID, notarized by Apple, a disk
# image to download, and an update the app's auto-updater accepts. Nothing is published unless you
# pass --publish.
#
#   scripts/release.sh 0.1.1 "What changed"            build, sign, notarize → dist/v0.1.1/
#   scripts/release.sh 0.1.1 "What changed" --publish  …and create the GitHub release v0.1.1
#
# Needs (once, on this Mac):
#   - a "Developer ID Application" certificate in your keychain (Xcode → Settings → Accounts)
#   - the update key at ~/.tauri/cue.key, its password in Keychain as "cue-updater-key"
#   - notarytool credentials saved as a keychain profile, "cue-notary" unless NOTARY_PROFILE says
#     otherwise (an app-specific password from appleid.apple.com):
#       xcrun notarytool store-credentials cue-notary --apple-id <you@example.com> --team-id <TEAM>
#     Without it the build is signed but not notarized (fine to try locally, not to publish).
#   - for --publish: gh, logged in, with push access to the repo below
# Local overrides (e.g. NOTARY_PROFILE=…) can go in .release.env at the repo root (git-ignored).
#
# Releases never include an add-on (src-tauri/ext): the build doesn't use the `ext` feature, and the
# script stops if the app contains any of its code anyway.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="spacegrowth/cue"
VERSION="${1:-}"; NOTES="${2:-}"; PUBLISH="${3:-}"
say() { printf '  %s\n' "$*"; }
die() { printf 'release: %s\n' "$*" >&2; exit 1; }
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "usage: scripts/release.sh <version, e.g. 0.1.1> \"notes\" [--publish]"
[ -n "$NOTES" ] || die "say what changed (it shows in Cue's update card)"
cd "$ROOT"
# shellcheck disable=SC1091
[ -f .release.env ] && . ./.release.env
PROFILE="${NOTARY_PROFILE:-cue-notary}"

# --- what signs it --------------------------------------------------------------------------------
IDENTITY="$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application: [^"]*\)".*/\1/p' | head -1)"
[ -n "$IDENTITY" ] || die "no Developer ID Application certificate in your keychain"
[ -f "$HOME/.tauri/cue.key" ] || die "no update key at ~/.tauri/cue.key"
KEY_PASSWORD="$(security find-generic-password -s cue-updater-key -w 2>/dev/null)" || die "no 'cue-updater-key' password in Keychain"
NOTARIZE=1
if ! xcrun notarytool history --keychain-profile "$PROFILE" >/dev/null 2>&1; then
  [ "$PUBLISH" = "--publish" ] && die "can't publish without notarizing: no working notarytool profile '$PROFILE' (see the top of this script)"
  NOTARIZE=0
  say "no notarytool profile '$PROFILE': signing only, not notarizing"
fi
notarize() {  # $1 = a zip or a dmg; waits for Apple's verdict
  local out
  out="$(xcrun notarytool submit "$1" --keychain-profile "$PROFILE" --wait 2>&1)" || true
  grep -q "status: Accepted" <<<"$out" || { printf '%s\n' "$out" >&2; die "Apple didn't accept $(basename "$1") (xcrun notarytool log <id> --keychain-profile $PROFILE says why)"; }
}

# --- the version, everywhere it's written -----------------------------------------------------------
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

# --- build: both chip types, signed with the hardened runtime -----------------------------------------
rustup target add x86_64-apple-darwin aarch64-apple-darwin >/dev/null
export TAURI_SIGNING_PRIVATE_KEY="$HOME/.tauri/cue.key" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$KEY_PASSWORD"
RELEASE_CONFIG='{"bundle":{"macOS":{"signingIdentity":"'"$IDENTITY"'","hardenedRuntime":true,"entitlements":"entitlements.plist"}}}'
say "building (both chip types; a few minutes)…"
npx tauri build --target universal-apple-darwin --bundles app --config "$RELEASE_CONFIG"
APP="src-tauri/target/universal-apple-darwin/release/bundle/macos/Cue.app"

# --- checks before anything leaves this Mac -----------------------------------------------------------
for trace in axum "cue://pair" jsonwebtoken; do
  [ "$(strings "$APP/Contents/MacOS/cue" | grep -c "$trace")" = 0 ] || die "the app contains add-on code ($trace): not releasing it"
done
codesign --verify --deep --strict "$APP" || die "the signature doesn't check out"
lipo -archs "$APP/Contents/MacOS/cue" | grep -q x86_64 || die "not a universal build"

DIST="$ROOT/dist/v$VERSION"; rm -rf "$DIST"; mkdir -p "$DIST"
WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT

# --- notarize the app and staple Apple's ticket to it ---------------------------------------------------
if [ "$NOTARIZE" = 1 ]; then
  say "notarizing the app…"
  ditto -c -k --keepParent "$APP" "$WORK/Cue.zip"
  notarize "$WORK/Cue.zip"
  xcrun stapler staple "$APP" >/dev/null || die "couldn't staple the app's notarization"
  spctl --assess --type execute "$APP" || die "Gatekeeper rejects the app"
fi

# --- the update: the (notarized) app as tar.gz, signed with the update key ------------------------------
tar -czf "$DIST/Cue.app.tar.gz" -C "$(dirname "$APP")" Cue.app
# (Without the build's key variables: the signer refuses a key given twice.)
env -u TAURI_SIGNING_PRIVATE_KEY -u TAURI_SIGNING_PRIVATE_KEY_PASSWORD npx tauri signer sign -f "$HOME/.tauri/cue.key" -p "$KEY_PASSWORD" "$DIST/Cue.app.tar.gz" >/dev/null
SIG="$(cat "$DIST/Cue.app.tar.gz.sig")"; rm "$DIST/Cue.app.tar.gz.sig"
URL="https://github.com/$REPO/releases/download/v$VERSION/Cue.app.tar.gz"
python3 - "$VERSION" "$NOTES" "$SIG" "$URL" "$DIST/latest.json" <<'PY'
import json, sys, datetime
v, notes, sig, url, out = sys.argv[1:]
both = {"signature": sig, "url": url}
json.dump({"version": v, "notes": notes, "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
           "platforms": {"darwin-aarch64": both, "darwin-x86_64": both}}, open(out, "w"), indent=2)
PY

# --- the download: a disk image (open it, drag Cue to Applications), signed and notarized too -----------
mkdir -p "$WORK/dmg"
ditto "$APP" "$WORK/dmg/Cue.app"
ln -s /Applications "$WORK/dmg/Applications"
DMG="$DIST/Cue-$VERSION.dmg"
hdiutil create -volname "Cue" -srcfolder "$WORK/dmg" -ov -format UDZO "$DMG" >/dev/null
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

say "ready    → $DIST"
ls -1 "$DIST" | sed 's/^/             /'
if [ "$PUBLISH" = "--publish" ]; then
  gh release create "v$VERSION" --repo "$REPO" --title "Cue $VERSION" --notes "$NOTES" \
    "$DMG" "$DIST/Cue.app.tar.gz" "$DIST/latest.json"
  say "published → https://github.com/$REPO/releases/tag/v$VERSION"
else
  say "not published (add --publish to create the GitHub release)"
fi
