{
  pkgs,
  src,
  tooling,
}:
let
  rustPlatform = pkgs.makeRustPlatform {
    cargo = tooling.rust;
    rustc = tooling.rust;
  };
  packagePkgs = pkgs // {
    inherit rustPlatform;
  };
  eutheto-cli = import ./eutheto-cli.nix {
    pkgs = packagePkgs;
    inherit src;
  };
in
{
  default = eutheto-cli;
  inherit eutheto-cli;
}
