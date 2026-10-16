#!/usr/bin/env bash
# Reuse the real X11 input/DB scenario against the Windows release in Wine.
set -euo pipefail
export CARGO_TARGET_DIR="${ROLLING_TIMER_HOST_TARGET_DIR:-target/cross-host}"
if [[ -z ${DISPLAY:-} ]]; then exec ./scripts/with-display.sh "$0" "$@"; fi
binary=${1:-target/x86_64-pc-windows-msvc/release/rolling-timer.exe}
binary=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve(strict=True))' "$binary")
export WINEPREFIX="${ROLLING_TIMER_WINEPREFIX:-$PWD/.scratch/wine-x86_64}"
mkdir -p .scratch
wrapper=$(mktemp .scratch/windows-runner.XXXXXX)
trap 'wineserver -k 2>/dev/null || true; rm -f "$wrapper"' EXIT
python3 - "$binary" "$PWD/scripts/run-wine.sh" "$wrapper" <<'PY'
import pathlib, shlex, sys
prefix = '#!/usr/bin/env bash\nset -euo pipefail\n'
prefix += 'runner=' + shlex.quote(sys.argv[2]) + '\n'
prefix += 'binary=' + shlex.quote(sys.argv[1]) + '\n'
# The host test owns a Unix temporary directory. Pass its Z: mapping to the
# guest so the native SQLite inspector and Windows app use the same file.
prefix += r'''
if [[ $# == 2 && $1 == --data-dir ]]; then
    directory="Z:${2//\//\\}"
    exec "$runner" "$binary" --data-dir "$directory"
fi
exec "$runner" "$binary" "$@"
'''
pathlib.Path(sys.argv[3]).write_text(prefix)
PY
chmod +x "$wrapper"
ROLLING_TIMER_TEST_BINARY="$PWD/$wrapper" ./scripts/gui-test.sh e2e
