{
  description = "Rolling Timer: reproducible Rust desktop development";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs = { nixpkgs, rust-overlay, ... }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      each = nixpkgs.lib.genAttrs systems;
      source = nixpkgs.lib.fileset.toSource {
        root = ./.;
        fileset = nixpkgs.lib.fileset.unions [ ./.cargo ./cmake ./Cargo.toml ./Cargo.lock ./src ./tests ./LICENSE ./README.md ];
      };
      environment = system:
        let
          pkgs = import nixpkgs { inherit system; overlays = [ rust-overlay.overlays.default ]; };
          toolchain = pkgs.rust-bin.stable."1.98.1".default.override { extensions = [ "rust-src" "llvm-tools" ]; };
          native = with pkgs; [ toolchain cmake ninja pkg-config just cargo-llvm-cov python3 ];
          libraries = with pkgs; lib.optionals stdenv.isLinux [
            xorg.libX11 xorg.libXext xorg.libXft xorg.libXinerama xorg.libXcursor xorg.libXrender xorg.libXfixes fontconfig freetype
          ];
          fontEnvironment = pkgs.lib.optionalAttrs pkgs.stdenv.isLinux {
            FONTCONFIG_FILE = pkgs.makeFontsConf { fontDirectories = [ pkgs.dejavu_fonts ]; };
          };
          guiTools = pkgs.lib.optionals pkgs.stdenv.isLinux [ pkgs.xorg.xorgserver pkgs.xdotool ];
        in { inherit pkgs toolchain native libraries fontEnvironment guiTools; };
    in {
      devShells = each (system:
        let e = environment system; in {
          default = e.pkgs.mkShell ({
            nativeBuildInputs = e.native ++ e.guiTools;
            buildInputs = e.libraries;
            RUST_BACKTRACE = "1";
            CARGO_BUILD_JOBS = "2";
          } // e.fontEnvironment);
          nightly = e.pkgs.mkShell ({
            nativeBuildInputs = [ (e.pkgs.rust-bin.nightly."2026-09-19".default.override {
              extensions = [ "rust-src" "miri" ];
            }) e.pkgs.clang ] ++ (builtins.tail e.native) ++ e.guiTools;
            buildInputs = e.libraries;
            CARGO_BUILD_JOBS = "2";
          } // e.fontEnvironment);
        } // e.pkgs.lib.optionalAttrs (system == "x86_64-linux") {
          # Cross tools are opt-in: normal development does not download Wine,
          # QEMU or Windows SDK tooling. No binfmt registration/root is needed.
          cross = e.pkgs.mkShell ({
            nativeBuildInputs = [
              (e.toolchain.override { targets = [
                "aarch64-unknown-linux-musl"
                "x86_64-pc-windows-msvc" "aarch64-pc-windows-msvc"
              ]; })
              # Backport upstream #452 to the pinned 0.20 tool: Rust 1.98
              # emits an AArch64 flag that Zig's driver cannot parse.
              # https://github.com/rust-cross/cargo-zigbuild/pull/452
              (e.pkgs.cargo-zigbuild.overrideAttrs (old: {
                postPatch = (old.postPatch or "") + ''
                  substituteInPlace src/zig.rs \
                    --replace-fail 'else if arg == "-Wl,-znostart-stop-gc" {' \
                      'else if arg == "-Wl,-znostart-stop-gc" || arg == "-Wl,--fix-cortex-a53-843419" {'
                '';
              }))
              e.pkgs.zig e.pkgs.cargo-xwin
              e.pkgs.qemu e.pkgs.wine64
              e.pkgs.clang e.pkgs.lld e.pkgs.llvmPackages.llvm
            ] ++ (builtins.tail e.native) ++ e.guiTools;
            buildInputs = e.libraries;
            CARGO_BUILD_JOBS = "2";
            WINEDLLOVERRIDES = "mscoree,mshtml=";
          } // e.fontEnvironment);
        });
      packages = each (system:
        let e = environment system; in {
          default = (e.pkgs.makeRustPlatform { cargo = e.toolchain; rustc = e.toolchain; }).buildRustPackage {
            pname = "rolling-timer";
            version = "0.1.0";
            src = source;
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = [ e.pkgs.cmake e.pkgs.ninja e.pkgs.pkg-config ];
            buildInputs = e.libraries;
            # GUI tests need a display; the pure/persistence integration suite does not.
            checkFlags = [ "--skip" "desktop_widgets" ];
          };
        } // e.pkgs.lib.optionalAttrs e.pkgs.stdenv.isLinux {
          static = e.pkgs.lib.makeOverridable (import ./nix/static-linux.nix) {
            inherit (e) pkgs toolchain; inherit source;
          };
        } // e.pkgs.lib.optionalAttrs (system == "x86_64-linux") {
          static-arm64 = import ./nix/static-linux.nix {
            inherit (e) pkgs toolchain;
            inherit source;
            staticPkgs = e.pkgs.pkgsCross.aarch64-multiplatform-musl.pkgsStatic;
            runner = "${e.pkgs.qemu}/bin/qemu-aarch64";
          };
        });
    };
}
