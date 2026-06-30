{
  description = "Rust DDC/CI backlight kernel module";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    systems.url = "github:nix-systems/default";

    pre-commit-hooks-nix = {
      url = "github:cachix/pre-commit-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { ... }@inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {

      systems = import inputs.systems;

      imports = [
        inputs.pre-commit-hooks-nix.flakeModule
      ];

      perSystem =
        {
          pkgs,
          config,
          ...
        }:
        {

          packages = rec {
            default = pkgs.callPackage ./package.nix { };
            ddci-backlight = default;
          };

          pre-commit = {
            check.enable = true;
            settings.hooks = {
              nixfmt-rfc-style.enable = true;
              deadnix.enable = true;
              statix.enable = true;
              rustfmt = {
                enable = true;
                package = pkgs.rustfmt;
                pass_filenames = true;
                entry = "rustfmt";
                args = [
                  "--edition=2021"
                ];
                files = "\\.rs$";
                types = [ "file" ];
              };
            };
          };

          devShells.default = pkgs.mkShell {
            inputsFrom = [
              config.pre-commit.devShell
            ];
          };
        };
    };
}
