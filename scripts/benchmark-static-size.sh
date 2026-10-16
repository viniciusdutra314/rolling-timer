#!/usr/bin/env bash
# Controlled comparisons of the current static build, with identical Rust code.
# Nix runs the release test suite for each variant. First runs can take minutes.
set -euo pipefail
mkdir -p .scratch
report=.scratch/static-size.csv
printf 'variant,bytes\n' > "$report"
# Freeze the dirty Git tree once: edits made while a long benchmark is running
# must not silently change the application source between variants.
snapshot=$(nix flake metadata --no-write-lock-file --json "git+file://$PWD" | python3 -c 'import json,sys; print(json.load(sys.stdin)["path"])')
export TIMER_SIZE_FLAKE="path:$snapshot"
for variant in compact native-objects full-sqlite rust-s; do
    export TIMER_SIZE_VARIANT=$variant
    nix build --impure --cores 2 --max-jobs 1 --expr '
      let
        f = builtins.getFlake (builtins.getEnv "TIMER_SIZE_FLAKE");
        release = f.packages.x86_64-linux.static;
        variant = builtins.getEnv "TIMER_SIZE_VARIANT";
      in if variant == "native-objects" then
        release.override { nativeSizeOptimization = false; }
      else if variant == "full-sqlite" then
        release.overrideAttrs (old: { env = old.env // { LIBSQLITE3_FLAGS = ""; }; })
      else if variant == "rust-s" then
        release.overrideAttrs (old: { env = old.env // { CARGO_PROFILE_RELEASE_OPT_LEVEL = "s"; }; })
      else release
    ' --out-link ".scratch/static-size-$variant" >".scratch/static-size-$variant.log" 2>&1
    printf '%s,%s\n' "$variant" "$(wc -c < ".scratch/static-size-$variant/bin/rolling-timer" | tr -d ' ')" >> "$report"
done
cat "$report"
