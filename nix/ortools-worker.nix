{ pkgs }:
let
  contract = {
    phase = "03";
    status = "blocked";
    available = false;
    requiredContract = [
      "exact-source-and-hash"
      "matched-protobuf"
      "reviewed-cmake-flags"
      "worker-smoke-tests"
      "license-and-solver-manifest"
    ];
  };
  blocked = builtins.derivation {
    name = "eutheto-ortools-worker-contract-blocked-phase-03";
    system = pkgs.stdenvNoCC.hostPlatform.system;
    builder = pkgs.runtimeShell;
    args = [
      "-e"
      "-c"
      ''
        echo "OR-Tools worker packaging is blocked: Phase 03 has not approved an exact source, hash, protobuf contract, build flags, or license manifest." >&2
        exit 1
      ''
    ];
    preferLocalBuild = true;
    allowSubstitutes = false;
  };
in
blocked
// {
  pname = "eutheto-ortools-worker";
  version = "unapproved-phase-03";
  inherit contract;
  passthru = { inherit contract; };
  meta = {
    description = "Unavailable OR-Tools worker contract pending Phase-03 approval";
    broken = true;
    license = pkgs.lib.licenses.asl20;
    platforms = [ ];
  };
}
