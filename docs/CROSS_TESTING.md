# Cross compilation and execution

From an x86-64 Linux host:

```sh
nix develop .#cross
just cross-linux-arm64
just cross-windows
just cross-windows-arm64-build
nix build .#static-arm64 --out-link .scratch/static-arm64 --cores 2 --max-jobs 1
./scripts/test-static-arm64.sh
```

The extra shell pins Rust targets, Zig/cargo-zigbuild, Clang/LLD/cargo-xwin,
Wine and QEMU through Nix. Normal development does not load these tools.
Microsoft's CRT and SDK are downloaded separately by cargo-xwin and are subject
to [its documented Microsoft license terms](https://github.com/rust-cross/cargo-xwin#prerequisite).
Their upstream selection is not locked by `flake.lock`; preserve the SDK cache
when reproducing an exact Windows toolchain. No Windows installation image is
downloaded or included in the repository.

| Target | Compilation | Execution from Linux |
| --- | --- | --- |
| Linux x86-64 | Cargo / Nix static release | Native tests, widgets and X11 process input |
| Linux ARM64 | cargo-zigbuild for core tests; Nix cross GCC for full static desktop | Explicit `qemu-aarch64`; no root or binfmt changes |
| Windows x86-64 MSVC | cargo-xwin, static CRT in Rust and FLTK | Wine: suite, widgets and release driven with real X11 input, checkpoints, failure/recovery, saved close and startup dialog |
| Windows ARM64 MSVC | cargo-xwin, executable and test executables | Compile-only locally; needs an ARM64 Windows guest for runtime validation |

The last row prints **not executed** explicitly. Compilation success is not a
substitute for an execution test. Actual local outcomes are in [VALIDATION.md](VALIDATION.md).

## Emulation boundaries

[QEMU user mode](https://www.qemu.org/docs/master/user/index.html) translates CPU
instructions and system calls for supported host/guest OS combinations; it does
not implement Windows APIs for a PE executable on Linux. Wine supplies those APIs
for the x86-64 Windows tests. Wine success is useful compatibility evidence and
does not replace the existing native Windows CI job.

The pinned Nixpkgs Wine 10 package only supports x86-64 hosts. It cannot run an
ARM64 Windows executable in the x86-64 Wine prefix. QEMU system emulation can boot
a complete guest, but needs a compatible, separately supplied Windows disk image
and firmware. This VM has no `/dev/kvm`; a guest here would use TCG CPU emulation.
Do not count the Windows ARM64 cell as passing until its executable/tests actually
run in that guest or on native Windows ARM64.

For an existing compatible x86-64 Windows qcow2 image, an isolated, nonpersistent
QEMU session can be started with the tools in the cross shell:

```sh
qemu-system-x86_64 -machine q35 -accel tcg -cpu max -smp 2 -m 4096 \
  -bios "$WINDOWS_UEFI_FIRMWARE" \
  -drive "file=$WINDOWS_QCOW2,format=qcow2" -snapshot \
  -nic none -display none -vnc 127.0.0.1:1
```

The firmware and image must match that machine. This is a manual guest-launch
example, **not a validated Windows VM test**. ARM64 needs matching AArch64
firmware, a compatible ARM machine/image and the ARM64 executable; changing only
the QEMU executable name is insufficient. No resident guest is started by Just/CI.

## Failures uncovered by cross testing

- Rust 1.98 passes an AArch64 linker argument unsupported by the pinned Zig
  driver. The Nix shell backports [cargo-zigbuild's upstream fix #452](https://github.com/rust-cross/cargo-zigbuild/pull/452).
  That tool is used for emulated core tests. The distributable ARM64 static build
  uses GCC/binutils and retains Rust's Cortex-A53 erratum linker option.
- FLTK has its own `FLTK_MSVC_RUNTIME_DLL` option, independent of the C wrapper's
  `static-msvcrt` feature. `cmake/fltk-toolchain.cmake` turns it off and chains the
  generated cross toolchain. Existing CMake build trees configured before this
  fix need `cargo clean --target x86_64-pc-windows-msvc -p fltk-sys` once.
- Cargo's runner applies to test executables, not their child processes. CLI
  tests inject the application path/runner explicitly, so ARM64 tests do not need
  a privileged system-wide binfmt handler and Wine receives a Windows path.
- Native process-test helpers have their own `target/cross-host` build tree:
  Clang from the cross shell must not reuse a CMake tree configured with GCC.
  The same process scenario drives both ARM64/QEMU and Windows/Wine releases;
  the Wine adapter maps its host temporary directory through the guest's Z: drive.
