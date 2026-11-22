{
  description = "A configurable system fetch with centered ASCII art and powerline panels";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    { self, nixpkgs, flake-utils }:
    let
      systems = flake-utils.lib.eachDefaultSystem (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          halofetch = pkgs.callPackage ./nix/package.nix { };
        in
        {
          packages = {
            default = halofetch;
            halofetch = halofetch;
          };

          apps.default = {
            type = "app";
            program = "${halofetch}/bin/halofetch";
            meta.description = "Run halofetch";
          };

          devShells.default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              clippy
              rustc
              rustfmt
            ];
          };
        }
      );
    in
    systems
    // {
      nixosModules = {
        default = ./nix/nixos.nix;
        halofetch = ./nix/nixos.nix;
      };

      homeManagerModules = {
        default = ./nix/home-manager.nix;
        halofetch = ./nix/home-manager.nix;
      };

      overlays.default = final: prev: {
        halofetch = final.callPackage ./nix/package.nix { };
      };
    };
}
