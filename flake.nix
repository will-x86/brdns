{
  description = "Basic rust flake :)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
      in
      with pkgs;
      {
        devShells.default = mkShell {
          LD_LIBRARY_PATH = lib.makeLibraryPath [ openssl ];
          buildInputs = [
            openssl
            pkg-config
            eza
            fd
            cargo
            rustc
            rustup
            rust-analyzer
            pkgs.zsh
          ];

          shellHook = ''
            alias ls=eza
            export PATH=$PATH:${pkgs.rust-analyzer}/bin
            alias find=fd
          '';
        };
      }
    );
}
