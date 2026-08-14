{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    ra-mux = {
      url = "github:dan-kc/ra-mux";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
    };
  };

  outputs =
    {
      nixpkgs,
      fenix,
      flake-utils,
      ra-mux,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ fenix.overlays.default ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        scripts = import ./scripts.nix { inherit pkgs; };
        retent = pkgs.rustPlatform.buildRustPackage {
          pname = "retent";
          version = "0.1.0";
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter =
              path: _type:
              builtins.baseNameOf path != "target" && builtins.baseNameOf path != "result";
          };
          cargoLock.lockFile = ./Cargo.lock;
        };
      in
      {
        packages.default = retent;
        checks.default = retent;

        devShells.default =
          with pkgs;
          mkShell {
            buildInputs = [
              (fenix.packages.${system}.complete.withComponents [
                "cargo"
                "clippy"
                "rust-src"
                "rustc"
                "rustfmt"
                "rust-analyzer"
              ])
              nil
              ra-mux.packages.${system}.default
            ]
            ++ scripts;
            shellHook = ''
              status
            '';
          };
      }
    );
}
