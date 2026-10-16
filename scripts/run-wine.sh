#!/usr/bin/env bash
# Cargo runner for x86-64 Windows tests. Keep Wine state out of the user's home.
set -euo pipefail
export WINEPREFIX="${ROLLING_TIMER_WINEPREFIX:-$PWD/.scratch/wine-x86_64}"
export WINEDEBUG="${WINEDEBUG:--all}"
export WINEDLLOVERRIDES='mscoree,mshtml='
exec wine64 "$@"
