#!/usr/bin/env bash
# Enter `nix develop .#cross` first. Compilation and execution are distinct gates.
set -euo pipefail
case ${1:-} in
    linux-arm64)
        target=aarch64-unknown-linux-musl
        mkdir -p .scratch
        # zigbuild builds test executables; QEMU runs them explicitly, without
        # registering a system-wide binfmt handler or assuming a guest sysroot.
        cargo zigbuild --locked --no-default-features --tests --target "$target" --message-format=json-render-diagnostics > .scratch/arm64-artifacts.jsonl
        python3 - <<'PY'
import json, pathlib, subprocess
artifacts = [json.loads(line) for line in pathlib.Path('.scratch/arm64-artifacts.jsonl').read_text().splitlines()]
tests = [a['executable'] for a in artifacts if a.get('reason') == 'compiler-artifact' and a.get('profile', {}).get('test') and a.get('executable')]
if not tests:
    raise SystemExit('No ARM64 test executables were produced')
for executable in tests:
    subprocess.run(['qemu-aarch64', executable, '--test-threads=1'], check=True, timeout=120)
PY
        ;;
    windows-x86_64)
        if [[ -z ${DISPLAY:-} ]]; then exec ./scripts/with-display.sh "$0" "$@"; fi
        export WINEPREFIX="${ROLLING_TIMER_WINEPREFIX:-$PWD/.scratch/wine-x86_64}"
        trap 'wineserver -k 2>/dev/null || true' EXIT
        export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER="$PWD/scripts/run-wine.sh"
        # The executable's path embedded by Cargo is a Linux path when cross
        # compiling. The CLI subprocess test receives an explicit guest path.
        debug_binary=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "${CARGO_TARGET_DIR:-target}/x86_64-pc-windows-msvc/debug/rolling-timer.exe")
        export ROLLING_TIMER_TEST_BINARY="Z:${debug_binary//\//\\}"
        cargo xwin test --locked --target x86_64-pc-windows-msvc -- --test-threads=1
        cargo xwin test --locked --target x86_64-pc-windows-msvc --test desktop -- --ignored --test-threads=1
        cargo xwin build --locked --release --target x86_64-pc-windows-msvc
        ./scripts/check-windows-imports.sh "${CARGO_TARGET_DIR:-target}/x86_64-pc-windows-msvc/release/rolling-timer.exe"
        ./scripts/run-wine.sh "${CARGO_TARGET_DIR:-target}/x86_64-pc-windows-msvc/release/rolling-timer.exe" --version
        ./scripts/test-windows-process.sh "${CARGO_TARGET_DIR:-target}/x86_64-pc-windows-msvc/release/rolling-timer.exe"
        ;;
    windows-arm64-build)
        cargo xwin build --locked --release --target aarch64-pc-windows-msvc
        cargo xwin test --locked --target aarch64-pc-windows-msvc --no-run
        ./scripts/check-windows-imports.sh "${CARGO_TARGET_DIR:-target}/aarch64-pc-windows-msvc/release/rolling-timer.exe"
        echo 'Windows ARM64: compilation passed; runtime requires an ARM64 Windows guest. Not executed.'
        ;;
    *) echo 'Usage: cross-test.sh linux-arm64 | windows-x86_64 | windows-arm64-build' >&2; exit 2 ;;
esac
