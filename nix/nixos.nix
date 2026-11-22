# NixOS module for halofetch. It installs the package and nothing else: shell
# startup hooks stay explicit through `halofetch init`, so enabling the module
# never edits a user's shell configuration.
{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.programs.halofetch;
in
{
  options.programs.halofetch = {
    enable = lib.mkEnableOption "halofetch, a configurable system fetch";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.callPackage ./package.nix { };
      defaultText = lib.literalMD "the `halofetch` package from this flake";
      description = ''
        The halofetch package to install.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];
  };
}
