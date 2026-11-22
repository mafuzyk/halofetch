# Home Manager module for halofetch. It installs the package and leaves shell
# startup hooks and the configuration file to the user, so enabling it never
# overwrites a configuration written by the editor.
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
    home.packages = [ cfg.package ];
  };
}
