{
  description = "Basic rust flake :)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
      in
      with pkgs;
      {
        devShells.default = mkShell {
          LD_LIBRARY_PATH = lib.makeLibraryPath [
            openssl
            gcc
            clang
          ];
          buildInputs = [
            openssl
            pkg-config
            gcc
            clang
            eza
            fd
            (rust-bin.nightly.latest.default.override {
              extensions = [
                "rust-src"
                "llvm-tools"
              ];
            })
            rust-analyzer
            # cargo-watch
            # pkgs.sqlite
            # pkgs.bunyan-rs
            pkgs.zsh
            pkgs.cmake # pingora
            pkgs.knot-dns
            pkgs.bazel_8
          ];

          shellHook = ''
            alias ls=eza
            alias find=fd
            export FUZZTEST_LIB_PATH="$PWD/fuzztest/bazel-bin/centipede"
            export FUZZTEST_CENTIPEDE_BINARY_PATH="$PWD/fuzztest/bazel-bin/centipede/centipede"

            if { [ ! -f "$FUZZTEST_LIB_PATH/libcentipede_engine_static.a" ] || [ ! -f "$FUZZTEST_CENTIPEDE_BINARY_PATH" ]; } && [ -f "$PWD/fuzztest/centipede/BUILD" ]; then
              echo "building centipede (one-time bazel build)"
              (cd "$PWD/fuzztest" && bazel build //centipede:centipede_engine_static //centipede:centipede)
            fi
          '';
        };
      }
    );
}
