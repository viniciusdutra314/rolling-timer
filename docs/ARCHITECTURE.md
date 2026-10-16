# Design and review notes

## Accounting contract

The app measures the union of half-open sessions `[start, end)` intersected with
`[now - window, now)`. A session exactly on either boundary contributes zero
outside the window. Overlapping/duplicate sessions count once. This intentionally
improves on the Python prototype's sum, which could double-count overlapping rows.
No session history is deleted when changing the window: increasing it can bring
older usage back into view.

Timestamps are UTC Unix milliseconds in 1970–9999. Window and optional quota are
positive, finite durations, rounded to the nearest millisecond when entered as
hours. Invalid timestamps, reversed intervals and zero windows cannot be
constructed through their public APIs. Quota is separate from window size;
remaining time saturates at zero when used time exceeds quota. The quota is an
estimate configured by the user, not a server-synchronized entitlement.

`domain::usage` sorts and merges intervals before measuring their intersections.
Its sum cannot overflow: disjoint pieces fit within the supported window, whose
maximum is below `i64::MAX`. There are no speculative data structures or interval
caches. The SQLite query filters expired/future records; the domain independently
clips every result. Property tests compare this algorithm against enumeration of
individual milliseconds, plus split/duplicate and monotonicity properties.

## State, clocks and failure atomicity

`Engine<Store>` owns a `Stopped | Recording { id, last }` state. The caller supplies
all timestamps. The domain and engine never read wall time, process arguments or
global settings. `Store` is the persistence boundary; tests inject failures around
a real SQLite store. Accounting callbacks enqueue actions; chart pointer callbacks only update their
copyable cursor state and request a repaint. Callbacks do no database work. The
single desktop event loop dispatches actions and displays their `Result` errors.

SQLite commits happen before publishing settings, recording state or a new
checkpoint. A failed stop/close keeps the app open and recording state available
for retry. Start checks saved history to reject a clock earlier than its latest
end. A backwards clock during a session rejects checkpoints rather than shortening
saved usage. Wait for the clock to catch up or correct the system clock to retry.

Wall time is intentional: time during sleep/suspend counts while recording, just
as in the prototype. Forward clock changes also count. UTC avoids daylight-saving
ambiguity but cannot prove how much real time passed during a clock correction.
An `Instant` only schedules screen refreshes; it does not determine usage.

Every one-second checkpoint uses SQLite FULL synchronous mode. An abrupt process
termination retains the last committed checkpoint, and reopening starts stopped.
Time after that checkpoint is not guessed. Thus abrupt death may lose the time
since the last successful tick (normally up to about one second, potentially more
when the process is suspended, storage is failing or the event loop is delayed).
There is no background daemon or automatic microphone detection.

## Persistence and migration

Both versions use `~/.rolling_timer/time_intervals.sqlite3`. Windows resolves the
home through `USERPROFILE`; Unix through `HOME`. `--data-dir PATH` allows isolated
profiles and removable-drive storage. An OS file lock is held for the store's
lifetime. Different directories are independent; a second process using the same
directory receives a recoverable error. The lock file stays on disk, but ownership
is released by the OS on process death. SQLite's locking remains in use as well.

Schema version 1 creates integer `sessions` and singleton `settings` tables.
SQLite CHECK constraints mirror application range invariants. At first open a
single transaction imports Python's `time_intervals` table and optional TOML
configuration, then sets `user_version=1`. RFC 3339 timestamps with explicit UTC
offsets are accepted; sub-millisecond timestamp fractions are truncated. A bad
row or invalid TOML rolls back the complete migration and reports the problem.
Newer schemas are rejected instead of guessed at.

The legacy table and TOML remain untouched during migration. Import runs once. Returning to Python
after migration does not synchronize new Python sessions into the Rust table.
Back up the original directory before switching versions permanently.

## Desktop and distribution choices

FLTK is a conventional desktop toolkit with desktop widgets, software rendering,
keyboard navigation and no webview/GPU runtime. Its C++ code and SQLite are built
from source and statically linked. Pango, image codecs and OpenGL are disabled;
this keeps the dependency tree small, at the cost of complex-script text support
on Linux. Application labels currently use English, as in the prototype.

The default development binary uses the platform's X11/font libraries on Linux.
The separate `nix build .#static` builds those libraries against musl and rejects
an ELF interpreter or NEEDED entries in the resulting executable. X11/XWayland,
a display connection and installed fonts are still runtime resources. The pinned C++ support archive requires a non-PIE static executable, so this
artifact does not provide PIE-based address randomization. A static
binary cannot create a desktop session on a machine that has none.

Windows builds enable static CRT in both Rust and FLTK. System Win32 DLLs remain
OS-provided APIs. macOS uses Cocoa and libSystem from the OS. Neither platform is
advertised as an executable with literally zero OS dynamic dependencies. Native
CI runners are necessary to validate each platform; a Linux pass is not evidence
that the macOS or Windows runner has passed.

References: [FLTK distribution and safety FAQ](https://fltk-rs.github.io/fltk-rs/FAQ.html),
[Rusqlite bundled SQLite](https://github.com/rusqlite/rusqlite#notes-on-building-rusqlite-and-libsqlite3-sys),
[Rust static CRT linkage](https://doc.rust-lang.org/reference/linkage.html),
[Apple SDK linking](https://developer.apple.com/library/archive/documentation/DeveloperTools/Conceptual/cross_development/Using/using.html).

## What “panic-free” and coverage mean here

All application targets forbid unsafe Rust and deny Clippy's unwrap, expect,
panic, indexing/slicing and unchecked time-subtraction lints. Errors from clocks,
validation, filesystem, SQL and desktop waiting use `Result`. Tests use assertions
and explicitly allow test-fixture `expect` calls. Release builds retain ordinary
panic behavior: `panic=abort` is not used to disguise a panic as success.

This is not a proof that the whole process can never panic, abort, corrupt memory
or run out of memory. Rust's allocator, standard library and dependencies are
outside such a guarantee. In particular FLTK's wrapper has internal assertions
and native code; SQLite also uses C. Miri checks our pure-domain executions;
AddressSanitizer exercises Rust and instrumented SQLite, and the separate GUI
recipe also instruments FLTK's C++ code with leak detection enabled. UBSan
instruments SQLite.
Those tools detect failures on executed paths; none proves their absence on all
inputs or all platforms.

Coverage is measured from instrumented Rust executions, including widget and
process tests. `just coverage` has a strict 100% line acceptance gate. Do not
confuse that target with branch coverage, dependency/native-code coverage or proof
of correctness. The measured result and any remaining gaps belong in
[VALIDATION.md](VALIDATION.md), even if the acceptance gate still fails.

## Themes and timeline graphs

`preferences::Theme` is a closed enum; its stable lowercase file identifier is
independent of displayed labels. Missing preferences use the original dark theme.
An unknown identifier, wrong TOML type or malformed file produces a visible error
rather than silently discarding it. The GUI receives a save callback: persistence
succeeds before publishing the new palette, and a failure restores the menu's
previous selection. Changing themes does not depend on the recording clock.

The TOML parser is shared by migration and preference editing. `toml_edit` keeps
comments and unrelated entries; the legacy window remains optional for fresh
profiles containing only `theme`. `tempfile` writes in the destination directory,
syncs the new file and atomically replaces the previous file, cleaning up after
failed publication. The existing profile lock covers these writes. This does not
claim that the directory rename itself survives every power failure.

`charts::Timeline` accepts intervals, settings and an injected timestamp. It clips
sessions to the present window, unions overlaps, and partitions the result into
at most 48 nonoverlapping history buckets. The sum of those bars equals the main
timer's used duration. Each bar measures usage within that bucket, not accumulated
usage since the beginning of the window.

A clipped session `[a,b)` starts expiring at `a + window` and finishes at
`b + window`. These endpoints, now and now + window define every change in slope
of the forecast. Between endpoints its slope is either zero or minus one, so the
forecast's interpolation is exact at millisecond resolution. The prediction assumes
no further usage, even when recording continues. The optional available-quota
readout saturates at zero independently of expired usage. Empty usage clears now;
otherwise the last merged endpoint plus the window is the exact clearing time.

An open graph refreshes from the same checkpoint as the main labels. Hidden
graphs do no extra database work; opening them never stops recording. Rendering
uses FLTK's existing software drawing primitives with the current palette, without
adding a chart framework. `chrono::Local` queries the OS timezone for each date;
we do not reuse today's fixed offset for historical/future timestamps. Storage
and interval arithmetic retain UTC instants. Local civil tick generation keeps
both occurrences of a repeated hour and omits nonexistent hours. An unavailable
refresh displays an error in both windows.

Tests compare forecasts with independently recomputed sliding-window usage,
including overlaps, future/expired data, epoch boundaries, long windows and quota
saturation. Widget tests cover every palette, rejected writes, live forecasts, direct plot inspection and
window reuse. Process tests exercise menu selection, TOML persistence, reopening,
chart controls, independent dismissal and quitting from the auxiliary window.

The calendar-axis presentation is in `chart_axes.rs`, independent of FLTK and
accepting an injected timezone. Hourly grid lines are distinct from the sparser
labels needed to avoid overlap; shorter and very long windows adapt their grid
spacing. Minute scales round upwards to integer 1/2/5 multiples. The forecast
renders used and released time as separate solid/dashed curves, with a clearing
marker. Both charts keep hover positions across periodic refreshes using `Cell`,
without callback borrowing or an extra slider. Date/clock labels have no decimal
hour notation. Tests cover crossing midnight, half-hour offsets, gaps/folds and
OS timezone lookup in subprocesses with isolated TZ settings.

Chrono's [Local documentation](https://docs.rs/chrono/latest/chrono/struct.Local.html)
describes the OS timezone adapter. The `clock` feature is enabled only for the
GUI; pure chart/domain tests keep clocks and timezone conversion injected.
