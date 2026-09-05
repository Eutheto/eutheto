{ pkgs, src }:
pkgs.rustPlatform.buildRustPackage {
  pname = "eutheto-cli";
  version = "0.1.0";

  inherit src;
  cargoLock.lockFile = src + "/Cargo.lock";

  cargoBuildFlags = [
    "--package"
    "eutheto-cli"
  ];
  # Integration fixtures require the debug-only pack; the installed build stays release.
  checkType = "debug";
  cargoTestFlags = [
    "--package"
    "eutheto-cli"
  ];

  strictDeps = true;

  meta = {
    description = "Phase-00 eutheto command-line interface (working executable name is non-final)";
    license = pkgs.lib.licenses.asl20;
    mainProgram = "optimizer";
    platforms = pkgs.lib.platforms.all;
  };
}
