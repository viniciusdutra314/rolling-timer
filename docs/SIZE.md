# Binary size measurements

These controlled comparisons predate themes and timeline graphs. Their Rust code
was held constant within each experiment; they are not the size of the expanded
application. See [the current feature record](THEMES_AND_CHARTS.md) for updated
artifact sizes.

Measured on 2026-10-16 with the locked Rust/Nix inputs. Sizes below are bytes of
the **uncompressed executable**, stripped in every case. Native library sources,
Rust application code and runtime behavior were held constant while varying the
listed settings. These are file-size measurements, not runtime-speed benchmarks.

| Experiment | Linux x86-64 static bytes |
| --- | ---: |
| Previous release: Rust `s`, fat LTO, one codegen unit, stripped | 5,803,488 |
| Only Rust `z` | 5,795,296 |
| Only minimal Cargo features | 5,803,440 |
| Only per-function/data sections in FLTK/C wrapper | 5,241,920 |
| Only unused SQLite extensions removed | 5,463,040 |
| FLTK sections plus runtime X11/font archives built with `-Os` and sections | 4,352,864 |
| All reductions combined, Rust `s` | 4,008,272 |
| **All reductions combined, Rust `z` (adopted)** | **4,004,176** |

The result is **31.0% smaller** than the previous static release. The final
tar.gz, including license notices, is **2,447,132 bytes**. There is no executable
packer or runtime decompression step. SQLite, FLTK and the same X11/font stack
remain statically linked. The target of an approximately 1 MB Linux executable
was not reached by these experiments.

## What changed and why

- Cargo already used stripping, fat LTO, one codegen unit and size optimization.
  Measured `z` against `s` both before and after native changes, then selected `z`.
  [Cargo documents](https://doc.rust-lang.org/cargo/reference/profiles.html#opt-level)
  that `z` is not necessarily the smallest option.
- Audited production features with `cargo tree --locked -e features --edges normal`.
  All direct production dependencies now disable default features explicitly.
  Chrono needs no features for RFC3339 parsing; thiserror uses core error support;
  TOML needs `parse` and `serde` for Table/Value, but not `std` or writing support.
  A compile experiment without TOML's `serde` failed, so that feature was restored.
  Rusqlite has no default features in this version; explicitly disabling defaults
  is future-facing configuration, not a claimed size gain. `hooks` stays test-only.
  The previous selection was already fairly small: this audit saved only 48 bytes.
- `bundled` SQLite's build script enables extensions independently of Cargo
  features. Disabled FTS3/5, RTree, DBSTAT, STAT4, SOUNDEX, JSON and extension
  loading. The app's private schema uses none of them. Kept mutexes, API armor,
  transactions, triggers, constraints and migrations. See the upstream
  [SQLite compile options](https://www.sqlite.org/compile.html).
- Rust already passes `--gc-sections`. Native archives previously grouped
  unrelated functions together. `-ffunction-sections -fdata-sections` makes them
  individually collectible; `-Os` also tunes the embedded runtime archives.
  These are the native counterparts of removing unreachable code. See
  [GCC's section options](https://gcc.gnu.org/onlinedocs/gcc/Optimize-Options.html)
  and [GNU ld garbage collection](https://sourceware.org/binutils/docs/ld/Options.html).

The final ELF inspection, process/UI scenario, migration tests and sanitizer
results are in [VALIDATION.md](VALIDATION.md). Removing SQL extensions does not
change the application database format. Loading unrelated third-party databases
that require those extensions is outside the timer's storage interface.

## Reproduce

```sh
nix develop -c just benchmark-static-size
```

This compares the current compact configuration with native object granularity,
the full bundled SQLite and Rust `s`, changing one setting at a time. It preserves
each artifact/log and writes `.scratch/static-size.csv`; Nix runs release tests
for each build. The first build can take several minutes because runtime archives
are recompiled. The historical feature-only measurement above is separate from
this current-build comparison. No application code is rewritten by the benchmark.

The current-build run measured `compact=4,004,176`,
`native-objects=5,454,800`, `full-sqlite=4,344,624`, and `rust-s=4,008,272` bytes. Unlike the historical
isolated experiments above, all other reductions remain enabled in these rows.
