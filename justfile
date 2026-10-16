# Enter `nix develop` (or allow direnv) before running recipes.
set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

fmt:
    cargo fmt --all -- --check

lint:
    cargo clippy --all-targets --all-features --locked -- -D warnings

test:
    cargo test --locked

gui-test:
    # Linux: use Xvfb without relying on a host desktop or a window manager.
    ./scripts/gui-test.sh

gui-e2e:
    ./scripts/gui-test.sh e2e

check: fmt lint test gui-test gui-e2e

coverage:
    mkdir -p .scratch/coverage
    # cargo-llvm-cov in the pinned Nixpkgs predates Cargo's cache-tag check.
    if [[ -d target/llvm-cov-target && ! -f target/llvm-cov-target/CACHEDIR.TAG ]]; then cp target/CACHEDIR.TAG target/llvm-cov-target/CACHEDIR.TAG; fi
    cargo llvm-cov clean --workspace
    cargo llvm-cov --all-features --locked --no-report
    ./scripts/gui-test.sh coverage
    ./scripts/gui-test.sh coverage-e2e
    cargo llvm-cov report --ignore-filename-regex '^/nix/store/' --lcov --output-path .scratch/coverage/lcov.info
    cargo llvm-cov report --ignore-filename-regex '^/nix/store/' --html --output-dir .scratch/coverage/html
    # This is an explicit acceptance gate, not a claim that it already passes.
    cargo llvm-cov report --ignore-filename-regex '^/nix/store/' --fail-under-lines 100

# Miri checks pure accounting and chart projections. The chart integration
# case uses SQLite FFI, so only that case is skipped.
miri:
    PROPTEST_CASES=8 MIRIFLAGS=-Zmiri-strict-provenance cargo miri test --no-default-features --test domain --test charts --test chart_axes -- --skip engine_chart_data_includes_active_checkpoint_and_does_not_stop_recording

# Run inside `nix develop .#nightly`; instrument Rust plus bundled SQLite.
asan:
    CARGO_TARGET_DIR=target/asan CC=clang CFLAGS='-fsanitize=address -fno-omit-frame-pointer' RUSTFLAGS='-Zsanitizer=address' RUSTDOCFLAGS='-Zsanitizer=address' cargo test -Zbuild-std --target x86_64-unknown-linux-gnu --no-default-features

asan-gui:
    CARGO_TARGET_DIR=target/asan-gui CC=clang CXX=clang++ CFLAGS='-fsanitize=address -fno-omit-frame-pointer' CXXFLAGS='-fsanitize=address -fno-omit-frame-pointer' RUSTFLAGS='-Zsanitizer=address' RUSTDOCFLAGS='-Zsanitizer=address' ./scripts/gui-test.sh asan

ubsan-sqlite:
    CARGO_TARGET_DIR=target/ubsan CFLAGS='-fsanitize=undefined -fno-sanitize-recover=all' RUSTFLAGS='-Clink-arg=-lubsan' cargo test --no-default-features --test storage

release:
    cargo build --release --locked

# Inspect actual dependencies/size; do not infer portability from successful linking.
inspect: release
    ./scripts/inspect-binary.sh target/release/rolling-timer

benchmark-size:
    ./scripts/benchmark-size.sh

benchmark-static-size:
    ./scripts/benchmark-static-size.sh

# Enter `nix develop .#cross` first; no root/binfmt changes are needed.
cross-linux-arm64:
    ./scripts/cross-test.sh linux-arm64

cross-windows:
    ./scripts/cross-test.sh windows-x86_64

cross-windows-arm64-build:
    ./scripts/cross-test.sh windows-arm64-build
