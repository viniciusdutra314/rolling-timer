#!/usr/bin/env bash
# Run any GUI command with a disposable X server when Linux has no display.
set -euo pipefail
if [[ $# == 0 ]]; then echo 'Usage: with-display.sh COMMAND [ARG...]' >&2; exit 2; fi
if [[ $(uname -s) != Linux || -n ${DISPLAY:-} ]]; then "$@"; exit; fi
mkdir -p .scratch
display_file=$(mktemp .scratch/xvfb-display.XXXXXX)
Xvfb -displayfd 3 -screen 0 1024x768x24 -nolisten tcp 3>"$display_file" >"$display_file.log" 2>&1 &
xvfb_pid=$!
trap 'kill "$xvfb_pid" 2>/dev/null || true; rm -f "$display_file" "$display_file.log"' EXIT
for _ in {1..100}; do
    if [[ -s $display_file ]]; then break; fi
    if ! kill -0 "$xvfb_pid" 2>/dev/null; then cat "$display_file.log" >&2; exit 1; fi
    sleep 0.05
done
if [[ ! -s $display_file ]]; then echo 'Xvfb did not start' >&2; exit 1; fi
export DISPLAY=":$(cat "$display_file")"
"$@"
