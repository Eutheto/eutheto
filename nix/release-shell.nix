{ pkgs, tooling }:
let
  releaseTooling = import ./release-tooling.nix { inherit pkgs; };
in
import ./dev-shell.nix {
  inherit pkgs tooling;
  shellName = "release";
  extraPackages = tooling.qualityTools ++ releaseTooling;
}
