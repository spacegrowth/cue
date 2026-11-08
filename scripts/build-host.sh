#!/bin/bash
# cue-host for the machines Cue reaches over SSH (docs/dev/cue-agent-host.md §3.4), into src-tauri/host-builds/:
#   cue-host-linux-x86_64, cue-host-linux-aarch64   static (musl), cross-built here with Rust's own linker
#   cue-host-darwin                                  Apple silicon + Intel
#   BUILD                                            the host build they are (host/build.rs's fingerprint)
#   cue-pi.ts                                        Cue's Pi extension (pi/cue.ts), for Pi on a machine
#   SHA256SUMS                                       what Cue checks a copy on a machine against
# Cue.app carries the folder (tauri.conf.json resources → Contents/Resources/host-builds) and copies the one a machine
# needs there. Nothing is rebuilt when the folder already holds this build's.
#
#   scripts/build-host.sh                build what's missing
#   scripts/build-host.sh --check        and run each Linux one in Docker (its `version` must say this build)
#   scripts/build-host.sh --sign <id>    the macOS one signed with that identity and the hardened runtime (a
#                                        release: notarization takes no unsigned program inside the app)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/src-tauri/host-builds"
LINUX=(x86_64 aarch64)
CHECK=0
SIGN=""
while [ $# -gt 0 ]; do
  case "$1" in
    --check) CHECK=1 ;;
    --sign) SIGN="${2:?--sign needs an identity}"; shift ;;
    *) echo "build-host: unknown option $1"; exit 2 ;;
  esac
  shift
done
mkdir -p "$OUT"
cd "$ROOT/src-tauri"

cargo build -q -p cue-host --release
BUILD="$(target/release/cue-host version | awk '{print $2}')"
[ -n "$BUILD" ] || { echo "build-host: cue-host didn't say its build"; exit 1; }

have() { [ -f "$OUT/BUILD" ] && [ "$(cat "$OUT/BUILD")" = "$BUILD" ] && [ -x "$OUT/$1" ]; }
built=1
for f in cue-host-darwin "${LINUX[@]/#/cue-host-linux-}"; do have "$f" || built=0; done

if [ "$built" = 0 ]; then
  # Another build's files go first, so the folder never mixes builds.
  rm -f "$OUT"/cue-host-* "$OUT/SHA256SUMS" "$OUT/BUILD"
  for t in aarch64-apple-darwin x86_64-apple-darwin; do cargo build -q -p cue-host --release --target "$t"; done
  lipo -create -output "$OUT/cue-host-darwin" target/aarch64-apple-darwin/release/cue-host target/x86_64-apple-darwin/release/cue-host
  # Linux: musl's own C library and start files come with Rust's target (self-contained), linked by rust-lld, so
  # no C toolchain for Linux is needed here. The host's crates are all Rust.
  for arch in "${LINUX[@]}"; do
    t="$arch-unknown-linux-musl"
    rustup target list --installed | grep -qx "$t" || rustup target add "$t"
    # Added to RUSTFLAGS (a release sets it, and it would win over a per-target setting); with --target it reaches
    # only the program, never build scripts, which run here.
    RUSTFLAGS="${RUSTFLAGS:-} -C linker=rust-lld -C linker-flavor=ld.lld -C link-self-contained=yes" cargo build -q -p cue-host --release --target "$t"
    cp "target/$t/release/cue-host" "$OUT/cue-host-linux-$arch"
  done
  chmod 755 "$OUT"/cue-host-*
fi
# Signing changes the file, so sums are taken after; a build signed before is signed again (the same identity, or
# a release's over a local one).
if [ -n "$SIGN" ]; then codesign --force --options runtime --timestamp --sign "$SIGN" "$OUT/cue-host-darwin"; fi
# Cue's Pi extension goes with it: Pi's gate, put on a machine whose Pi doesn't have it (hosted/machine_host.rs).
cp "$ROOT/pi/cue.ts" "$OUT/cue-pi.ts"
(cd "$OUT" && shasum -a 256 cue-host-* cue-pi.ts >SHA256SUMS)
echo "$BUILD" >"$OUT/BUILD"
echo "  cue-host $BUILD: $(cd "$OUT" && ls cue-host-* cue-pi.ts | tr '\n' ' ')"

[ "$CHECK" = 1 ] || exit 0
# Docker as it's set up here, without a credential helper that may not be installed (none is needed to pull a
# public image), and the files under your home (Docker on a Mac shares that with its VM, not /var/folders).
DOCKER_HOST="$(docker context inspect --format '{{.Endpoints.docker.Host}}')"
DOCKER_CONFIG="$(mktemp -d)"
export DOCKER_HOST DOCKER_CONFIG
trap 'rm -rf "$DOCKER_CONFIG"' EXIT
echo '{}' >"$DOCKER_CONFIG/config.json"
for arch in "${LINUX[@]}"; do
  platform="linux/$([ "$arch" = x86_64 ] && echo amd64 || echo arm64)"
  said="$(docker run --rm --platform "$platform" -v "$OUT:/b:ro" alpine:3.20 /b/cue-host-linux-"$arch" version)"
  [ "$(echo "$said" | awk '{print $2}')" = "$BUILD" ] || { echo "build-host: the Linux $arch build says '$said', not $BUILD"; exit 1; }
  echo "  runs on Linux $arch ($platform): $said"
done
