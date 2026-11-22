# Source package shared by the flake output, the overlays and both modules.
{
  lib,
  rustPlatform,
}:

let
  src = ../.;
  cargoToml = lib.importTOML (src + "/Cargo.toml");
in
rustPlatform.buildRustPackage {
  pname = "halofetch";
  version = cargoToml.package.version;
  inherit src;

  cargoLock = {
    lockFile = src + "/Cargo.lock";
  };

  postInstall = ''
    install -Dm644 ${src}/LICENSE $out/share/licenses/halofetch/LICENSE
    install -Dm644 ${src}/THIRD_PARTY_NOTICES.md $out/share/licenses/halofetch/THIRD_PARTY_NOTICES.md
    install -Dm644 ${src}/README.md $out/share/doc/halofetch/README.md
    install -Dm644 ${src}/DOCS.md $out/share/doc/halofetch/DOCS.md
    install -Dm644 ${src}/logos/README.md $out/share/doc/halofetch/logo-provenance.md
  '';

  meta = {
    description = "A configurable system fetch with centered ASCII art and powerline panels";
    longDescription = ''
      halofetch displays system information next to a colored ASCII or Kitty
      image logo, with three responsive scenes, a TUI editor, a live monitor and
      a PTY-backed shell. It is a single Rust binary with embedded logos.
    '';
    homepage = "https://github.com/mafuzyk/halofetch";
    license = lib.licenses.gpl3Plus;
    platforms = lib.platforms.linux;
    mainProgram = "halofetch";
  };
}
