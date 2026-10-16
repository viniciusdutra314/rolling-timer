# Rolling Timer

A tiny, fully functional **sliding-window time tracker** written in Python.

Traditional clock apps such as [GNOME Clocks](https://apps.gnome.org/en/Clocks/) and [KDE Clock](https://apps.kde.org/kclock/) provide stopwatches and countdown timers, but don't offer rolling-window usage tracking as a documented feature. Rolling Timer fills that little gap: it shows **how much time you've used in the last N hours**.

![](image.png)

Press **Start** to track a session and **Stop** to save it. The total updates automatically—even while stopped, as older sessions leave the window. You can change the window size directly in the app.

## Requirements

- **Python 3.12+** with `tkinter` and sqlite3 support (generally it comes pre-installed).

The app is designed to work on **Linux, Windows, and macOS**, wherever Tkinter is available.

## Run

```sh
python3 rolling_timer.py
```

Sessions are stored in a SQLite3 database and the window setting in TOML, under `~/.rolling_timer/` (your home directory).

## Rust development shell

The Nix flake provides Rust/Cargo 1.91.0, rustfmt, and Clippy on Linux
(x86_64 and ARM64) and macOS (Apple Silicon only), using nixpkgs 26.05 stable.
Nix must have `nix-command` and `flakes` enabled.

```sh
nix develop
cargo --version
```

If you use direnv with Nix support, run `direnv allow` to load the shell automatically.

## Philosophy

One Python file, roughly 200 lines, and only the standard library. The point is to be **simple, useful, and easy to hack on**
