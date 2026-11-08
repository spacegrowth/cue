#!/bin/bash
# Hosted sessions on Linux machines, for real: Cue's tests (src-tauri/src/hosted/machine_tests.rs) against Docker
# containers running sshd (tests/machines/), reached with real ssh. cue-host is built for Linux first and put on each
# container by Cue itself, as it would put it on your machines. The stand-in agent is the tests' fake Claude.
#
#   scripts/test-machines.sh              on this Mac's own architecture (arm64 on Apple silicon)
#   scripts/test-machines.sh --both       and on x86_64 (emulated: slower)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PLATFORMS="linux/$(uname -m | sed 's/x86_64/amd64/; s/aarch64/arm64/')"
[ "${1:-}" = "--both" ] && PLATFORMS="linux/arm64 linux/amd64"

"$ROOT/scripts/build-host.sh"

# Docker as it's set up here, without a credential helper that may not be installed.
DOCKER_HOST="$(docker context inspect --format '{{.Endpoints.docker.Host}}')"
DOCKER_CONFIG="$(mktemp -d)"
export DOCKER_HOST DOCKER_CONFIG
trap 'rm -rf "$DOCKER_CONFIG"' EXIT
echo '{}' >"$DOCKER_CONFIG/config.json"

# Each platform's machine image, made by running tests/machines/setup.sh in a plain Ubuntu container of that platform
# and saving the result (not a Dockerfile: Docker's classic builder can't build for another platform here). Made
# again only when setup.sh or start.sh change (their checksum is the image's label).
SETUP="$(cat "$ROOT/tests/machines/setup.sh" "$ROOT/tests/machines/start.sh" | shasum -a 256 | cut -c1-16)"
for p in $PLATFORMS; do
  arch="${p#linux/}"
  tag="cue-test-machine:$arch"
  [ "$(docker image inspect --format '{{index .Config.Labels "cue.setup"}}' "$tag" 2>/dev/null)" = "$SETUP" ] && continue
  echo "  making $tag (a minute or two; longer for a platform that's emulated)"
  docker rm -f "cue-test-setup-$arch" >/dev/null 2>&1 || true
  docker run --platform "$p" --name "cue-test-setup-$arch" -v "$ROOT/tests/machines:/setup:ro" ubuntu:24.04 sh /setup/setup.sh
  docker commit --change 'CMD ["/start.sh"]' --change "LABEL cue.setup=$SETUP" "cue-test-setup-$arch" "$tag" >/dev/null
  docker rm "cue-test-setup-$arch" >/dev/null
done

cd "$ROOT/src-tauri"
# One test at a time: they share the ssh stand-in (testing::FakeSsh), and each starts its own container.
env -u CUE_HOSTED CUE_TEST_MACHINES="$PLATFORMS" cargo test --lib hosted::machine_tests -- --ignored --test-threads=1 "${@:2}"
