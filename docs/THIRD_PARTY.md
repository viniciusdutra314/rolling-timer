# Third-party software

The application is MIT licensed. The exact Rust dependency versions and checksums
are recorded in `Cargo.lock`. Their licenses remain with their respective authors.
The desktop releases embed SQLite and FLTK, including native components.

- [Rusqlite](https://github.com/rusqlite/rusqlite): MIT; includes
  [SQLite](https://www.sqlite.org/copyright.html), dedicated to the public domain.
- [fltk-rs](https://github.com/fltk-rs/fltk-rs): MIT.
- [FLTK](https://www.fltk.org/COPYING.php): LGPL-2.0-or-later with the FLTK static
  linking exception; see the upstream full text. Image libraries are disabled.
- [Chrono](https://github.com/chronotope/chrono),
  [toml_edit](https://github.com/toml-rs/toml),
  [tempfile](https://github.com/Stebalien/tempfile),
  [thiserror](https://github.com/dtolnay/thiserror): MIT or Apache-2.0.
- The Linux static package additionally embeds musl (MIT), X.Org client libraries
  (permissive X11/MIT licenses), Fontconfig (MIT), FreeType (FreeType License or
  GPL-2.0; distributed under the FreeType License), and their static dependencies.
  FreeType portions are copyright The FreeType Project (https://freetype.org/).

Full Rust license texts can be collected with `scripts/collect-licenses.py` after
`cargo fetch --locked`. The Nix static output also installs available native
license texts in `share/rolling-timer/licenses`. Include those notices when
redistributing binaries; this document is a component index, not a replacement
for their license texts.
