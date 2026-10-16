# A true musl static executable, including X11 and font libraries. The ordinary
# Cargo build deliberately remains convenient for platform-native development.
{ pkgs, toolchain, source, staticPkgs ? pkgs.pkgsStatic, runner ? null,
  nativeSizeOptimization ? true }:
let
  static = staticPkgs;
  # Rust already uses section GC. Native archives need one section per function
  # and datum too, or one reachable function retains its entire object file.
  # Tune only runtime archives; a global overlay would rebuild the toolchain.
  tune = package: if !nativeSizeOptimization then package else package.overrideAttrs (old:
    let flags = " -Os -ffunction-sections -fdata-sections"; in
    if (old.env or {}) ? NIX_CFLAGS_COMPILE then {
      env = old.env // { NIX_CFLAGS_COMPILE = old.env.NIX_CFLAGS_COMPILE + flags; };
    } else { NIX_CFLAGS_COMPILE = (old.NIX_CFLAGS_COMPILE or "") + flags; });
  rustTarget = static.stdenv.hostPlatform.rust.rustcTarget;
  rust = toolchain.override { targets = [ rustTarget ]; };
  # System configuration paths must survive copying the binary off a Nix host.
  fontconfig = tune (static.fontconfig.overrideAttrs (old: {
    postPatch = (old.postPatch or "") + ''
      substituteInPlace src/fccfg.c \
        --replace-fail 'dir = (FcChar8 *) FONTCONFIG_PATH;' 'dir = (FcChar8 *) "/etc/fonts";'
    '';
    configureFlags = (old.configureFlags or []) ++ [
      "--with-default-fonts=/usr/share/fonts" "--with-add-fonts=/usr/local/share/fonts"
    ];
  }));
  xft = tune (static.xorg.libXft.override { inherit fontconfig; });
  xlibs = (with static; map tune [
    xorg.libX11 xorg.libXext xorg.libXinerama xorg.libXcursor
    xorg.libXrender xorg.libXfixes freetype
  ]) ++ [ fontconfig xft ];
  cfltk = static.stdenv.mkDerivation {
    pname = "cfltk-static";
    version = "1.5.23";
    src = pkgs.fetchurl {
      name = "fltk-sys-1.5.23.tar.gz";
      url = "https://static.crates.io/crates/fltk-sys/fltk-sys-1.5.23.crate";
      hash = "sha256-bZyZ4pRYkjq+Pwe3+KO8W13josBK2CJz/ZZAKzZm7QY=";
    };
    sourceRoot = "fltk-sys-1.5.23/cfltk";
    nativeBuildInputs = with pkgs; [ cmake ninja pkg-config ];
    buildInputs = xlibs;
    # Upstream's pkg-config file unconditionally lists disabled renderers. Make
    # the metadata describe this X11/no-Pango/no-Wayland build precisely.
    postPatch = ''
      # Nix paths cannot contain spaces. Unquoted .pc values also avoid escaped
      # quote characters in upstream's simple pkg-config output parser.
      sed -i 's/"//g' cfltk.pc.in
      substituteInPlace cmake/cfltk-pkgconfig.cmake \
        --replace-fail ' x11 xinerama xcursor xfixes xext xrender xft fontconfig cairo pango pangoxft pangocairo gobject-2.0 wayland-client wayland-cursor xkbcommon' \
                       ' x11 xinerama xcursor xfixes xext xrender xft fontconfig' \
        --replace-fail '-ldbus-1 -lsupc++' '-lsupc++'
    '';
    postInstall = ''
      mkdir -p "$out/share/rolling-timer/licenses/cfltk"
      cd "$NIX_BUILD_TOP/$sourceRoot"
      find . -type f \( -iname 'COPYING*' -o -iname 'LICENSE*' -o -iname 'NOTICE*' \) \
        -exec cp --parents '{}' "$out/share/rolling-timer/licenses/cfltk" \;
    '';
    NIX_CFLAGS_COMPILE = pkgs.lib.optionalString nativeSizeOptimization "-ffunction-sections -fdata-sections";
    cmakeFlags = [
      "-DCMAKE_BUILD_TYPE=MinSizeRel"
      "-DCFLTK_LINK_IMAGES=OFF" "-DCFLTK_USE_OPENGL=OFF"
      "-DFLTK_USE_PANGO=OFF" "-DFLTK_GRAPHICS_CAIRO=OFF"
      "-DFLTK_BACKEND_WAYLAND=OFF" "-DFLTK_BUILD_GL=OFF"
      "-DFLTK_BUILD_EXAMPLES=OFF" "-DFLTK_BUILD_TEST=OFF"
      "-DFLTK_BUILD_FLUID=OFF" "-DFLTK_BUILD_FLTK_OPTIONS=OFF"
      "-DFLTK_BUILD_HTML_DOCS=OFF" "-DFLTK_BUILD_PDF_DOCS=OFF"
    ];
  };
  # Only the final target uses musl. Build scripts/proc macros must keep the
  # native glibc compiler and stdlib; replacing their stdenv mixes the two ABIs.
  pkgConfigLibraries = xlibs ++ (with static; map tune [
    xorg.libxcb xorg.libXau xorg.libXdmcp xorg.libpthreadstubs
    libpng zlib bzip2 brotli expat
  ]) ++ [ static.xorg.xorgproto ];
  nativeNotices = pkgs.runCommand "rolling-timer-native-notices" { nativeBuildInputs = [ pkgs.python3 ]; } ''
    python3 ${../scripts/collect-licenses.py} --native "$out" ${pkgs.lib.concatMapStringsSep " "
      (library: pkgs.lib.escapeShellArg "${library.pname or library.name}=${library.src}")
      (pkgs.lib.unique (pkgConfigLibraries ++ [ static.stdenv.cc.libc static.stdenv.cc.cc ]))}
  '';
  targetEnv = pkgs.lib.toUpper (builtins.replaceStrings [ "-" ] [ "_" ] rustTarget);
  targetCc = "${static.stdenv.cc}/bin/${static.stdenv.cc.targetPrefix}cc";
in
(pkgs.makeRustPlatform { cargo = rust; rustc = rust; }).buildRustPackage {
  pname = "rolling-timer-static";
  version = "0.1.0";
  src = source;
  cargoLock.lockFile = ../Cargo.lock;
  nativeBuildInputs = [ pkgs.pkg-config ];
  env = {
    "CARGO_TARGET_${targetEnv}_LINKER" = targetCc;
    # Nix's static libsupc++ archive is not PIC; use a static non-PIE executable.
    "CARGO_TARGET_${targetEnv}_RUSTFLAGS" = "-C relocation-model=static";
    "CC_${builtins.replaceStrings [ "-" ] [ "_" ] rustTarget}" = targetCc;
    PKG_CONFIG_PATH = "${cfltk}/share/pkgconfig:"
      + pkgs.lib.makeSearchPathOutput "dev" "lib/pkgconfig" pkgConfigLibraries + ":"
      + pkgs.lib.makeSearchPathOutput "dev" "share/pkgconfig" pkgConfigLibraries;
  } // pkgs.lib.optionalAttrs (runner != null) {
    "CARGO_TARGET_${targetEnv}_RUNNER" = runner;
    # Test executables can run through Cargo's runner, but subprocesses they
    # spawn need the runner injected separately (no host binfmt registration).
    ROLLING_TIMER_TEST_RUNNER = runner;
  };
  preBuild = "export PKG_CONFIG_ALLOW_CROSS=1";
  buildPhase = ''
    runHook preBuild
    # Upstream fltk-sys silently ignores pkg-config failure; reject it early.
    pkg-config --libs --static cfltk
    cargo build --offline --locked --release --target ${rustTarget} --features fltk/pkg-config -j "$NIX_BUILD_CORES"
    runHook postBuild
  '';
  checkPhase = ''
    runHook preCheck
    cargo test --offline --locked --release --target ${rustTarget} --features fltk/pkg-config -j "$NIX_BUILD_CORES"
    runHook postCheck
  '';
  installPhase = ''
    runHook preInstall
    mkdir -p "$out/bin" "$out/share/rolling-timer/licenses"
    cp target/${rustTarget}/release/rolling-timer "$out/bin/"
    cp -r ${cfltk}/share/rolling-timer/licenses/cfltk "$out/share/rolling-timer/licenses/"
    cp -r ${nativeNotices} "$out/share/rolling-timer/licenses/native"
    runHook postInstall
  '';
  # Fail the build if the executable still needs an ELF interpreter or any DSO.
  postInstall = ''
    if ${pkgs.binutils}/bin/readelf -l "$out/bin/rolling-timer" | grep -q INTERP; then
      echo 'Unexpected dynamic interpreter in static release' >&2
      exit 1
    fi
    if ${pkgs.binutils}/bin/readelf -d "$out/bin/rolling-timer" | grep -q NEEDED; then
      echo 'Unexpected dynamic library in static release' >&2
      exit 1
    fi
  '';
}
