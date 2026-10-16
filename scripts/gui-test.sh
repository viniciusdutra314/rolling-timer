#!/usr/bin/env bash
set -euo pipefail
run_test() {
    if [[ ${1:-} == asan ]]; then
        cargo test --locked -Zbuild-std --target x86_64-unknown-linux-gnu --test desktop -- --ignored --test-threads=1
    elif [[ ${1:-} == coverage-e2e ]]; then
        cargo llvm-cov --all-features --locked --no-report --test process_desktop -- --ignored --test-threads=1
    elif [[ ${1:-} == coverage ]]; then
        cargo llvm-cov --all-features --locked --no-report --test desktop -- --ignored --test-threads=1
    elif [[ ${1:-} == e2e ]]; then
        cargo test --locked --test process_desktop -- --ignored --test-threads=1
    else
        cargo test --locked --test desktop -- --ignored --test-threads=1
    fi
}
if [[ $(uname -s) == Linux && -z ${DISPLAY:-} ]]; then
    exec ./scripts/with-display.sh "$0" "$@"
fi
run_test "${1:-}"
