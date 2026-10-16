#!/usr/bin/env bash
# Drive the ARM64 release through host X11 input, with explicit QEMU execution.
set -euo pipefail
# Cross shell native helpers use Clang; default development uses GCC. Keep
# CMake's cached compiler in its own build tree when driving foreign binaries.
export CARGO_TARGET_DIR="${ROLLING_TIMER_HOST_TARGET_DIR:-target/cross-host}"
binary=${1:-.scratch/static-arm64/bin/rolling-timer}
binary=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve(strict=True))' "$binary")
qemu-aarch64 "$binary" --version
mkdir -p .scratch
wrapper=$(mktemp .scratch/arm64-runner.XXXXXX)
trap 'rm -f "$wrapper"' EXIT
# shlex quotes paths as shell syntax; neither filenames nor environment text
# become executable shell fragments through string interpolation.
python3 - "$binary" "$wrapper" <<'PY'
import pathlib, shlex, sys
pathlib.Path(sys.argv[2]).write_text('#!/bin/sh\nexec qemu-aarch64 ' + shlex.quote(sys.argv[1]) + ' "$@"\n')
PY
chmod +x "$wrapper"
ROLLING_TIMER_TEST_BINARY="$PWD/$wrapper" ./scripts/gui-test.sh e2e
