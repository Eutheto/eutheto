{
  pkgs,
  src,
  protobuf,
}:
pkgs.rustPlatform.buildRustPackage {
  pname = "eutheto-solver-artifact-tool";
  version = "0.1.0";

  inherit src;
  cargoLock.lockFile = src + "/Cargo.lock";
  cargoBuildFlags = [
    "--package"
    "xtask"
  ];
  doCheck = false;

  strictDeps = true;
  nativeBuildInputs = [ protobuf ];
  PROTOC = "${protobuf}/bin/protoc";
  PROTOC_INCLUDE = "${protobuf}/include";

  postInstall = ''
    test "$("$PROTOC" --version)" = "libprotoc 33.1"
    test -x "$out/bin/xtask"
  '';
}
