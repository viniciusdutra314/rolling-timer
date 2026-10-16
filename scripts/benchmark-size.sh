#!/usr/bin/env bash
# Compare stripped binaries with Cargo's conventional optimization settings and
# this project's compact profile. No claim about runtime speed follows from size.
set -euo pipefail
mkdir -p .scratch
report=.scratch/binary-size.csv
printf 'profile,bytes,build_seconds\n' > "$report"
start=$(date +%s)
CARGO_TARGET_DIR=target/size-baseline CARGO_PROFILE_RELEASE_OPT_LEVEL=3 CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 cargo build --release --locked
binary=target/size-baseline/release/rolling-timer
if [[ -f $binary.exe ]]; then binary=$binary.exe; fi
printf 'stripped-baseline,%s,%s\n' "$(wc -c < "$binary" | tr -d ' ')" "$(( $(date +%s) - start ))" >> "$report"
start=$(date +%s)
CARGO_TARGET_DIR=target/size-compact cargo build --release --locked
binary=target/size-compact/release/rolling-timer
if [[ -f $binary.exe ]]; then binary=$binary.exe; fi
printf 'compact,%s,%s\n' "$(wc -c < "$binary" | tr -d ' ')" "$(( $(date +%s) - start ))" >> "$report"
cat "$report"
