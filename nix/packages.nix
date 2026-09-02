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
  supportsOrtoolsWorker = pkgs.stdenv.hostPlatform.system != "aarch64-linux";
  ortools-worker = import ./ortools-worker.nix {
    inherit pkgs src;
  };
in
{
  default = eutheto-cli;
  inherit eutheto-cli;
}
// pkgs.lib.optionalAttrs supportsOrtoolsWorker {
  inherit ortools-worker;
}
// pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
  desktop-runtime = tooling.desktopRuntime;
}
