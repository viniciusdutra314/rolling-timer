# Validation record

This is the earlier portability/size baseline, before themes and timeline graphs.
See [the current feature validation](THEMES_AND_CHARTS.md) for the new coverage,
Windows checks and the remaining LeakSanitizer finding.

Executed on Linux x86-64 in the pinned Nix environments, 2026-10-15/16
(America/Sao_Paulo). Windows x86-64 MSVC tests now also run locally under Wine;
this is compatibility testing, not a native Windows installation. Windows ARM64
release/test executables compile locally; their runtime has not been tested.
Native Windows/macOS CI has not been executed here. No GitHub release was published.

## Gates actually executed

| Check | Result |
| --- | --- |
| rustfmt + Clippy, all targets/features, warnings denied | Passed |
| Domain, engine, CLI, SQLite, migration and failure injection | Passed |
| Desktop widgets on the OS main thread, under Xvfb | Passed |
| Actual executable: X11 mouse/keyboard, idle checkpoints, invalid input, database failure/recovery, graceful close, startup-error dialog | Passed |
| Rust line coverage, including GUI/process scenarios | **567/567 lines, 100%** |
| Rust function coverage | **70/70 functions, 100%** |
| Rust region coverage | **959/982 regions, 97.66%** |
| Miri, strict provenance, pure-domain tests including fixed property cases | Passed |
| AddressSanitizer: Rust + bundled SQLite | Passed |
| AddressSanitizer + LeakSanitizer: Rust + SQLite + FLTK C++ widgets | Passed, leak detection enabled |
| UBSan: SQLite storage and migration tests | Passed |
| Linux musl release, embedded X11/font libraries, no ELF interpreter or dynamic section | Passed |
| Minimal Cargo features and reduced bundled SQLite: checks, full coverage, UBSan, ASan/LSan GUI | Passed |
| Linux ARM64 core/domain/SQLite/migration tests through cargo-zigbuild + QEMU | Passed |
| Linux ARM64 full static release and release integration suite, including CLI subprocesses through QEMU | Passed |
| Actual Linux ARM64 release: X11 input, checkpoints, injected SQLite failure/recovery and saved close through QEMU | Passed |
| Windows x86-64 MSVC through Wine: suite, CLI child processes, widget scenario, release launch | Passed |
| Actual Windows x86-64 release in Wine: X11 input, checkpoints, injected SQLite failure/recovery, saved close and startup dialog | Passed |
| Windows ARM64 MSVC: release and all test executables | Compiled; runtime not executed |
| Both Windows release PE import tables | No external CRT DLL imports |

The Windows x86-64 executable is 1,726,976 bytes; the ARM64 executable is
1,671,168 bytes. The Linux ARM64 static executable is 4,170,608 bytes. Different
OS backends make these cross-platform sizes unsuitable as an optimization
comparison; the controlled Linux comparison is below.

Coverage includes every Rust application module; no application file/function is
excluded. Test code and dependency/toolchain code are outside this percentage.
The region metric and unmeasured branch coverage are deliberately distinguished
from line coverage. The 100% line gate is enforced by `just coverage` and CI.

The SQLite failure tests include authorizer-denied writes/transactions/version
updates, commit-hook failure, malformed legacy column types, corrupt rows, schema
conflicts, invalid settings, missing session IDs, backward clocks and file errors.
The process scenario creates a SQLite trigger that fails a checkpoint, observes
that failure through its durable marker, removes it, and waits for recording to
resume before closing. It does not merely sleep and assume the checkpoint ran.

GUI tests use a custom harness because Cocoa requires widget construction on the
OS main thread. Normal `cargo test` explicitly skips the display scenario;
`just gui-test` opts in and runs it. Process-level X11 automation applies to Linux.

## Findings corrected during verification

- FLTK's `wait_for` returning false means an idle timeout, not a closed window.
  The executable now keeps checkpointing when no input events arrive.
- Constructor labels in fltk-rs 1.5.23 allocate C strings without reclaiming them.
  The adapter constructs with no label and uses the owned `with_label` copy path.
- Dropping Rust widget handles does not delete the root C++ window. The desktop
  adapter deletes its root and drains deferred callback cleanup on its UI thread.
  The fully instrumented GUI scenario passes with leak detection enabled.
- Successful checkpoint retries clear transient errors on screen; invalid-input
  feedback persists until the user takes an action.
- The musl C++ support archive is not PIC. The Linux artifact uses static non-PIE
  linkage, documented in the design notes rather than silently advertised as PIE.
- The static Fontconfig build looks for host configuration/fonts rather than
  assuming Nix store paths exist on the recipient's machine.
- Nix's read-only license directories must become writable in the disposable
  packaging copy. The packaging script now finishes successfully and cleans that
  copy after creating the archive; the Nix store itself stays untouched.
- Windows cross testing exposed FLTK's separate runtime-DLL option. The CMake
  policy now embeds the CRT in both FLTK and its wrapper; PE imports are checked.
  Details and emulator boundaries are in [CROSS_TESTING.md](CROSS_TESTING.md).

## Concrete size comparison

`scripts/benchmark-size.sh` compares equally stripped native Linux builds. Only
Rust opt level, LTO and codegen-unit settings differ; the toolkit's matching CMake
build type follows the profile. These are size measurements, not runtime-speed
claims. The final measurement reused dependency builds.

| Profile | Bytes |
| --- | ---: |
| Conventional stripped release (`opt-level=3`, LTO off, 16 units) | 3,574,224 |
| Compact release (`opt-level=s`, LTO on, 1 unit) | 2,213,368 |

This earlier native comparison used the original `s` profile. It still uses
dynamic Linux desktop libraries. The subsequent controlled **static** comparison
reduced the distributable from 5,803,488 to **4,004,176 bytes (31.0%)** by tuning
native archive sections, SQLite extensions, Cargo features and Rust `z`.
See the full experiments and reproduction command in [SIZE.md](SIZE.md).

## Final Linux archive

The local `rolling-timer-x86_64-unknown-linux-musl.tar.gz` contains the static
executable, application license, third-party guide and full Rust/native notices.
The executable is **4,004,176 bytes**; the archive is **2,447,132 bytes**.
Its SHA-256 is
`a481f2f44d674e9a69b4c672a4db35b5fe0b0d09384b4f41d4ce5a4f1e37e6de`.

Verified the checksum, extracted executable permissions, required notices and
byte-for-byte equality with the Nix build. The extracted executable runs
`--version` with an empty environment and passes the complete X11 process test,
including failure/recovery and saved close. ELF inspection confirms static
linkage and no dynamic dependencies. This artifact was created locally; it has
not been uploaded to GitHub.

## Reproduce

```sh
nix develop -c just check coverage ubsan-sqlite benchmark-size
nix develop .#nightly -c just miri asan asan-gui
nix build .#static
nix develop -c env ROLLING_TIMER_TEST_BINARY="$PWD/result/bin/rolling-timer" ./scripts/gui-test.sh e2e
```

Local raw logs, size CSV, full HTML/LCOV reports and the package experiment are
under ignored `.scratch/`. Coverage HTML: `.scratch/coverage/html/html/index.html`.
A real widget capture is committed as [desktop.png](desktop.png).

## Limits of the evidence

Line/function coverage does not prove panic freedom, absence of undefined behavior
or all failure modes. The application denies unsafe Rust and common panic-producing
operations, but dependencies/allocators/OS APIs can still fail outside those
constraints. Miri does not execute the SQLite/FLTK FFI paths; the native sanitizers
exercise those paths instead. Neither approach proves every dependency safe.

Wine execution does not replace native Windows validation. Windows/macOS need
their native CI runs before claiming release validation there.
The workflows create draft releases, preserve executable permissions in archives,
and include complete notices/checksums. macOS signing/notarization and a native
Wayland-only backend are not implemented.
