{
  description = "A PipeWire patchbay for Linux that visualizes your audio graph, hosts LV2 effects plugins inline, and auto-connects ports with persistent routing rules.";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    fenix.url = "github:nix-community/fenix/monthly";
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
  };
  outputs = {
    self,
    nixpkgs,
    flake-parts,
    ...
  } @ inputs:
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = [
        "aarch64-linux"
        "x86_64-linux"
      ];
      perSystem = {
        self',
        config,
        pkgs,
        ...
      }: let
        toolchain = inputs.fenix.packages.${pkgs.stdenv.hostPlatform.system}.default.toolchain;
      in {
        packages.default = config.packages.zestbay;
        packages.zestbay = pkgs.callPackage ./build.nix {
          rustPlatform = pkgs.makeRustPlatform {
            cargo = toolchain;
            rustc = toolchain;
          };
        };

        formatter = pkgs.alejandra;

        devShells.default = pkgs.mkShell {
          inputsFrom = [config.packages.default];
        };
      };
    };
}
