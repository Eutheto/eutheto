{
  pkgs,
  tooling,
  shellName,
  extraPackages ? [ ],
}:
let
  inherit (pkgs) lib;
  llvm = pkgs.llvmPackages;
  rustToolchain = tooling.rust;
  shellTools = lib.unique (tooling.executableTools ++ tooling.platformTools ++ extraPackages);
  shellLibraries = lib.unique (tooling.commonLibraries ++ tooling.platformLibraries);
  pkgConfigPackages = lib.unique (
    [
      pkgs.openssl
      pkgs.sqlite
      pkgs.protobuf
    ]
    ++ tooling.platformLibraries
  );
  linuxRuntimePackages = lib.unique (
    [
      pkgs.openssl
      pkgs.sqlite
      llvm.libclang
    ]
    ++ tooling.platformLibraries
  );
in
pkgs.mkShell {
  name = "eutheto-${shellName}";
  strictDeps = true;
  packages = shellTools;
  buildInputs = shellLibraries;

  EUTHETO_SHELL = shellName;

  RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
  LIBCLANG_PATH = "${llvm.libclang.lib}/lib";
  CLANG_PATH = "${llvm.clang}/bin/clang";
  PROTOC = "${pkgs.protobuf}/bin/protoc";
  PROTOC_INCLUDE = "${pkgs.protobuf}/include";
  CMAKE_GENERATOR = "Ninja";
  CMAKE_PREFIX_PATH = lib.concatStringsSep ":" [
    "${pkgs.openssl.dev}"
    "${pkgs.sqlite.dev}"
    "${pkgs.protobuf}"
  ];
  PKG_CONFIG_PATH = lib.concatStringsSep ":" [
    (lib.makeSearchPath "lib/pkgconfig" pkgConfigPackages)
    (lib.makeSearchPath "share/pkgconfig" pkgConfigPackages)
  ];
  OPENSSL_NO_VENDOR = "1";
  OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
  OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
  SQLITE3_LIB_DIR = "${pkgs.sqlite.out}/lib";
  SQLITE3_INCLUDE_DIR = "${pkgs.sqlite.dev}/include";
  LD_LIBRARY_PATH = lib.optionalString pkgs.stdenv.hostPlatform.isLinux (
    lib.makeLibraryPath linuxRuntimePackages
  );

  shellHook = ''
    export CARGO_HOME="$PWD/.cache/cargo"
    export CARGO_TARGET_DIR="$PWD/.cache/cargo-target"
    export COREPACK_HOME="$PWD/.cache/corepack"
    export NPM_CONFIG_CACHE="$PWD/.cache/npm"
    export NPM_CONFIG_STORE_DIR="$PWD/.cache/pnpm/store"
    export PNPM_HOME="$PWD/.cache/pnpm/home"
    export XDG_CACHE_HOME="$PWD/.cache"

    source ${./dev-shell-welcome.sh}
  '';
}
