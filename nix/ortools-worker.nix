{
  pkgs,
  src,
}:
let
  inherit (pkgs) lib stdenv;

  contractPath = ../workers/ortools/source-contract.json;
  repositoryPatchPath = ../workers/ortools/patches/9.15-candidate-fixes.patch;
  protocolSchemaPath = ../protocol/solver-worker.proto;
  contract = builtins.fromJSON (builtins.readFile contractPath);

  supportedSystems = [
    "x86_64-linux"
    "x86_64-darwin"
    "aarch64-darwin"
  ];
  linuxRuntimeLibraryPath = lib.makeLibraryPath [ stdenv.cc.cc.lib ];

  cmakeValue = value: if builtins.isBool value then if value then "ON" else "OFF" else toString value;
  commonCmakeFlags = lib.mapAttrsToList (
    name: value: "-D${name}=${cmakeValue value}"
  ) contract.cmake.cache_entries;
  commonCmakeFlagsShell = lib.escapeShellArgs commonCmakeFlags;
  outputPath = builtins.placeholder "out";
  darwinCmakeFlagsShell = lib.escapeShellArgs (
    lib.optionals stdenv.hostPlatform.isDarwin [
      "-DCMAKE_INSTALL_NAME_DIR=@rpath"
      "-DCMAKE_INSTALL_RPATH=${outputPath}/lib"
      "-DCMAKE_MACOSX_RPATH=ON"
    ]
  );

  fetchArchive =
    {
      name,
      url,
      sha256,
    }:
    pkgs.fetchurl {
      inherit name url sha256;
    };

  ortoolsSource = fetchArchive {
    name = "or-tools-v9.15.tar.gz";
    url = contract.ortools.source_url;
    sha256 = contract.ortools.sha256;
  };
  protobufSource = fetchArchive {
    name = "protobuf-33.1.tar.gz";
    url = contract.protobuf.source_url;
    sha256 = contract.protobuf.sha256;
  };
  zlibSource = fetchArchive {
    name = "zlib-v1.3.1.tar.gz";
    url = "https://github.com/madler/zlib/archive/refs/tags/v1.3.1.tar.gz";
    sha256 = "17e88863f3600672ab49182f217281b6fc4d3c762bde361935e436a95214d05c";
  };
  bzip2Source = fetchArchive {
    name = "bzip2-66c46b8c9436613fd81bc5d03f63a61933a4dcc3.tar.gz";
    url = "https://gitlab.com/bzip2/bzip2/-/archive/66c46b8c9436613fd81bc5d03f63a61933a4dcc3/bzip2-66c46b8c9436613fd81bc5d03f63a61933a4dcc3.tar.gz";
    sha256 = "3a4cff5f9d197e9e6c6138660afa6b1f9370df0bed135bd949243f6dfc83b3e1";
  };
  abseilSource = fetchArchive {
    name = "abseil-cpp-20250814.1.tar.gz";
    url = "https://github.com/abseil/abseil-cpp/archive/refs/tags/20250814.1.tar.gz";
    sha256 = "1692f77d1739bacf3f94337188b78583cf09bab7e420d2dc6c5605a4f86785a1";
  };
  re2Source = fetchArchive {
    name = "re2-2025-08-12.tar.gz";
    url = "https://github.com/google/re2/archive/refs/tags/2025-08-12.tar.gz";
    sha256 = "2f3bec634c3e51ea1faf0d441e0a8718b73ef758d7020175ed7e352df3f6ae12";
  };
  eigenSource = fetchArchive {
    name = "eigen-3.4.0.tar.gz";
    url = "https://gitlab.com/libeigen/eigen/-/archive/3.4.0/eigen-3.4.0.tar.gz";
    sha256 = "8586084f71f9bde545ee7fa6d00288b264a2b7ac3607b974e54d13e7162c1c72";
  };
in
assert contract.schema_version == 1;
assert contract.approval.phase == 3;
assert contract.approval.status == "approved";
assert contract.ortools.version == "9.15.6755";
assert contract.ortools.patch_path == "workers/ortools/patches/9.15-candidate-fixes.patch";
assert builtins.hashFile "sha256" repositoryPatchPath == contract.ortools.patch_sha256;
assert contract.protobuf.source_version == "33.1";
assert contract.protobuf.protoc_version == "33.1";
assert contract.protobuf.cpp_runtime_version == "33.1.0";
assert contract.protocol.wire_version == 1;
assert builtins.hashFile "sha256" protocolSchemaPath == contract.protocol.schema_sha256;
assert contract.worker.identity == "eutheto-ortools-worker";
assert contract.worker.version == "0.1.0";
assert builtins.elem stdenv.hostPlatform.system supportedSystems;
stdenv.mkDerivation {
  pname = contract.worker.identity;
  version = contract.worker.version;

  strictDeps = true;
  OR_TOOLS_PATCH = "6755";

  nativeBuildInputs = [
    pkgs.cmake
    pkgs.git
    pkgs.gnutar
    pkgs.gzip
    pkgs.ninja
    pkgs.pkg-config
    pkgs.python3
  ]
  ++ lib.optionals stdenv.hostPlatform.isLinux [ pkgs.patchelf ]
  ++ lib.optionals stdenv.hostPlatform.isDarwin [ pkgs.darwin.cctools ];
  buildInputs = lib.optionals stdenv.hostPlatform.isLinux [ stdenv.cc.cc.lib ];

  unpackPhase = ''
    runHook preUnpack

    mkdir -p "$NIX_BUILD_TOP/sources"
    tar -xzf ${ortoolsSource} -C "$NIX_BUILD_TOP/sources"
    tar -xzf ${protobufSource} -C "$NIX_BUILD_TOP/sources"
    tar -xzf ${zlibSource} -C "$NIX_BUILD_TOP/sources"
    tar -xzf ${bzip2Source} -C "$NIX_BUILD_TOP/sources"
    tar -xzf ${abseilSource} -C "$NIX_BUILD_TOP/sources"
    tar -xzf ${re2Source} -C "$NIX_BUILD_TOP/sources"
    tar -xzf ${eigenSource} -C "$NIX_BUILD_TOP/sources"

    sourceRoot="$NIX_BUILD_TOP/sources/or-tools-9.15"
    test -d "$sourceRoot"
    test -d "$NIX_BUILD_TOP/sources/protobuf-33.1"
    test -d "$NIX_BUILD_TOP/sources/zlib-1.3.1"
    test -d "$NIX_BUILD_TOP/sources/bzip2-66c46b8c9436613fd81bc5d03f63a61933a4dcc3"
    test -d "$NIX_BUILD_TOP/sources/abseil-cpp-20250814.1"
    test -d "$NIX_BUILD_TOP/sources/re2-2025-08-12"
    test -d "$NIX_BUILD_TOP/sources/eigen-3.4.0"
    cd "$sourceRoot"

    runHook postUnpack
  '';

  patchPhase = ''
    runHook prePatch

    git apply --check ${src}/workers/ortools/patches/9.15-candidate-fixes.patch
    git apply ${src}/workers/ortools/patches/9.15-candidate-fixes.patch

    (
      cd "$NIX_BUILD_TOP/sources/zlib-1.3.1"
      git apply --check --ignore-whitespace "$sourceRoot/patches/ZLIB-v1.3.1.patch"
      git apply --ignore-whitespace "$sourceRoot/patches/ZLIB-v1.3.1.patch"
    )
    (
      cd "$NIX_BUILD_TOP/sources/bzip2-66c46b8c9436613fd81bc5d03f63a61933a4dcc3"
      git apply --check --ignore-whitespace "$sourceRoot/patches/bzip2.patch"
      git apply --ignore-whitespace "$sourceRoot/patches/bzip2.patch"
    )
    (
      cd "$NIX_BUILD_TOP/sources/abseil-cpp-20250814.1"
      git apply --check --ignore-whitespace "$sourceRoot/patches/abseil-cpp-20250814.1.patch"
      git apply --ignore-whitespace "$sourceRoot/patches/abseil-cpp-20250814.1.patch"
    )
    (
      cd "$NIX_BUILD_TOP/sources/re2-2025-08-12"
      git apply --check --ignore-whitespace "$sourceRoot/patches/re2-2025-08-12.patch"
      git apply --ignore-whitespace "$sourceRoot/patches/re2-2025-08-12.patch"
    )
    (
      cd "$NIX_BUILD_TOP/sources/eigen-3.4.0"
      git apply --check --ignore-whitespace "$sourceRoot/patches/eigen3-3.4.0.patch"
      git apply --ignore-whitespace "$sourceRoot/patches/eigen3-3.4.0.patch"
    )

    runHook postPatch
  '';

  configurePhase = ''
    runHook preConfigure

    cmake \
      -S "$sourceRoot" \
      -B "$NIX_BUILD_TOP/ortools-build" \
      -G Ninja \
      ${commonCmakeFlagsShell} \
      ${darwinCmakeFlagsShell} \
      "-DCMAKE_CXX_FLAGS=-DEIGEN_MPL2_ONLY" \
      "-DCMAKE_INSTALL_PREFIX=$NIX_BUILD_TOP/ortools-stage" \
      "-DFETCHCONTENT_FULLY_DISCONNECTED=ON" \
      "-DFETCHCONTENT_SOURCE_DIR_ZLIB=$NIX_BUILD_TOP/sources/zlib-1.3.1" \
      "-DFETCHCONTENT_SOURCE_DIR_BZIP2=$NIX_BUILD_TOP/sources/bzip2-66c46b8c9436613fd81bc5d03f63a61933a4dcc3" \
      "-DFETCHCONTENT_SOURCE_DIR_ABSL=$NIX_BUILD_TOP/sources/abseil-cpp-20250814.1" \
      "-DFETCHCONTENT_SOURCE_DIR_PROTOBUF=$NIX_BUILD_TOP/sources/protobuf-33.1" \
      "-DFETCHCONTENT_SOURCE_DIR_RE2=$NIX_BUILD_TOP/sources/re2-2025-08-12" \
      "-DFETCHCONTENT_SOURCE_DIR_EIGEN3=$NIX_BUILD_TOP/sources/eigen-3.4.0"

    runHook postConfigure
  '';

  buildPhase = ''
    runHook preBuild

    cmake --build "$NIX_BUILD_TOP/ortools-build" --parallel "$NIX_BUILD_CORES"
    cmake --install "$NIX_BUILD_TOP/ortools-build"

    cmake \
      -S ${src}/workers/ortools \
      -B "$NIX_BUILD_TOP/worker-build" \
      -G Ninja \
      ${commonCmakeFlagsShell} \
      ${darwinCmakeFlagsShell} \
      "-DCMAKE_PREFIX_PATH=$NIX_BUILD_TOP/ortools-stage" \
      "-DEUTHETO_ORTOOLS_DEVELOPMENT_BUILD=OFF" \
      "-DEUTHETO_ORTOOLS_BUILD_TESTS=ON" \
      "-DEUTHETO_ORTOOLS_BUILD_CANDIDATE_BENCHMARKS=OFF" \
      "-DEUTHETO_ORTOOLS_PHASE3_CONTRACT=${src}/workers/ortools/source-contract.json"
    cmake --build "$NIX_BUILD_TOP/worker-build" --parallel "$NIX_BUILD_CORES"

    runHook postBuild
  '';

  doCheck = stdenv.buildPlatform.canExecute stdenv.hostPlatform;
  checkPhase = ''
    runHook preCheck

    ctest \
      --test-dir "$NIX_BUILD_TOP/worker-build" \
      --output-on-failure \
      --no-tests=error

    runHook postCheck
  '';

  installPhase = ''
    runHook preInstall

    cmake --install "$NIX_BUILD_TOP/worker-build" --prefix "$out"
    test -x "$out/bin/ortools-worker"

    runtime_dependencies() {
      if ${if stdenv.hostPlatform.isLinux then "true" else "false"}; then
        patchelf --print-needed "$1"
      else
        otool -L "$1" | awk 'NR > 1 { print $1 }'
      fi
    }

    is_forbidden_dependency() {
      case "$1" in
        *GLPK*|*glpk*|*gurobi*|*Gurobi*|*cplex*|*CPLEX*|*xpress*|*XPRESS*|\
        *python*|*Python*|*libjvm*|*java*|*Java*|*coreclr*|*hostfxr*|\
        *dotnet*|*DotNet*)
          return 0
          ;;
      esac
      return 1
    }

    is_system_dependency() {
      case "$1" in
        libc.so.*|libm.so.*|libdl.so.*|libpthread.so.*|librt.so.*|\
        libresolv.so.*|libutil.so.*|libstdc++.so.*|libgcc_s.so.*|\
        libatomic.so.*|libgomp.so.*|libquadmath.so.*|ld-linux*.so.*|\
        linux-vdso.so.*|/usr/lib/*|/System/Library/*|/nix/store/*)
          return 0
          ;;
      esac
      return 1
    }

    runtimeQueue="$NIX_BUILD_TOP/runtime-library-queue"
    runtimeSeen="$NIX_BUILD_TOP/runtime-library-seen"
    runtime_dependencies "$out/bin/ortools-worker" >"$runtimeQueue"
    : >"$runtimeSeen"
    mkdir -p "$out/lib"
    runtimeLibraryCount=0

    while IFS= read -r dependency; do
      test -n "$dependency" || continue
      if is_forbidden_dependency "$dependency"; then
        echo "forbidden worker runtime dependency detected: $dependency" >&2
        exit 1
      fi
      is_system_dependency "$dependency" && continue

      runtimeName="$(basename "$dependency")"
      grep -Fqx "$runtimeName" "$runtimeSeen" && continue
      printf '%s\n' "$runtimeName" >>"$runtimeSeen"

      sourceLibrary=
      for libraryDirectory in \
        "$NIX_BUILD_TOP/ortools-stage/lib" \
        "$NIX_BUILD_TOP/ortools-stage/lib64"
      do
        if test -e "$libraryDirectory/$runtimeName"; then
          sourceLibrary="$libraryDirectory/$runtimeName"
          break
        fi
      done
      if test -z "$sourceLibrary"; then
        echo "worker runtime dependency is not in the staged closure: $dependency" >&2
        exit 1
      fi

      currentLibrary="$sourceLibrary"
      while test -L "$currentLibrary"; do
        linkTarget="$(readlink "$currentLibrary")"
        case "$linkTarget" in
          /*|*/*)
            echo "runtime library has a non-local symlink target: $currentLibrary -> $linkTarget" >&2
            exit 1
            ;;
        esac
        cp -a "$currentLibrary" "$out/lib/"
        currentLibrary="$(dirname "$currentLibrary")/$linkTarget"
        if ! test -e "$currentLibrary"; then
          echo "runtime library symlink is unresolved: $sourceLibrary" >&2
          exit 1
        fi
      done

      cp -a "$currentLibrary" "$out/lib/"
      runtime_dependencies "$currentLibrary" >>"$runtimeQueue"
      runtimeLibraryCount=$((runtimeLibraryCount + 1))
    done <"$runtimeQueue"

    if test "$runtimeLibraryCount" -eq 0; then
      echo "worker runtime library closure is empty" >&2
      exit 1
    fi

    rm -rf "$NIX_BUILD_TOP/ortools-stage"

    runHook postInstall
  '';

  postFixup = lib.optionalString stdenv.hostPlatform.isLinux ''
    for binary in "$out/bin/ortools-worker" "$out"/lib/*.so "$out"/lib/*.so.*; do
      test -e "$binary" || continue
      test -L "$binary" && continue
      patchelf --set-rpath "$out/lib:${linuxRuntimeLibraryPath}" "$binary"
    done
  '';

  doInstallCheck = stdenv.buildPlatform.canExecute stdenv.hostPlatform;
  installCheckPhase = ''
    runHook preInstallCheck

    set +e
    "$out/bin/ortools-worker" </dev/null >worker.stdout 2>worker.stderr
    workerStatus=$?
    set -e
    if test "$workerStatus" -ne 64; then
      echo "installed worker returned $workerStatus for empty stdin; expected 64" >&2
      cat worker.stderr >&2
      exit 1
    fi
    if test -s worker.stdout; then
      echo "installed worker wrote protocol output for empty stdin" >&2
      cat worker.stdout >&2
      exit 1
    fi

    runtime_dependencies() {
      if ${if stdenv.hostPlatform.isLinux then "true" else "false"}; then
        patchelf --print-needed "$1"
      else
        otool -L "$1" | awk 'NR > 1 { print $1 }'
      fi
    }

    runtime_paths() {
      if ${if stdenv.hostPlatform.isLinux then "true" else "false"}; then
        patchelf --print-rpath "$1"
      else
        otool -l "$1" | awk '
          $1 == "cmd" && $2 == "LC_RPATH" { in_rpath = 1; next }
          in_rpath && $1 == "path" { print $2; in_rpath = 0 }
        '
      fi
    }

    is_forbidden_dependency() {
      case "$1" in
        *GLPK*|*glpk*|*gurobi*|*Gurobi*|*cplex*|*CPLEX*|*xpress*|*XPRESS*|\
        *python*|*Python*|*libjvm*|*java*|*Java*|*coreclr*|*hostfxr*|\
        *dotnet*|*DotNet*)
          return 0
          ;;
      esac
      return 1
    }

    is_system_dependency() {
      case "$1" in
        libc.so.*|libm.so.*|libdl.so.*|libpthread.so.*|librt.so.*|\
        libresolv.so.*|libutil.so.*|libstdc++.so.*|libgcc_s.so.*|\
        libatomic.so.*|libgomp.so.*|libquadmath.so.*|ld-linux*.so.*|\
        linux-vdso.so.*|/usr/lib/*|/System/Library/*|/nix/store/*)
          return 0
          ;;
      esac
      return 1
    }

    for artifact in "$out/bin/ortools-worker" "$out"/lib/*; do
      if test -L "$artifact"; then
        linkTarget="$(readlink "$artifact")"
        case "$linkTarget" in
          /*|*/*)
            echo "installed runtime symlink has a non-local target: $artifact -> $linkTarget" >&2
            exit 1
            ;;
        esac
        if ! test -e "$artifact"; then
          echo "installed runtime symlink is unresolved: $artifact" >&2
          exit 1
        fi
        continue
      fi

      linkage="$(runtime_dependencies "$artifact")"
      runtimePaths="$(runtime_paths "$artifact")"
      if is_forbidden_dependency "$linkage"; then
        echo "forbidden runtime dependency detected in $artifact" >&2
        printf '%s\n' "$linkage" >&2
        exit 1
      fi
      case "$runtimePaths" in
        *"$NIX_BUILD_TOP"*|*/build/*)
          echo "runtime loader metadata references the private build tree: $artifact" >&2
          printf '%s\n' "$runtimePaths" >&2
          exit 1
          ;;
      esac

      while IFS= read -r dependency; do
        test -n "$dependency" || continue
        is_system_dependency "$dependency" && continue
        runtimeName="$(basename "$dependency")"
        if ! test -e "$out/lib/$runtimeName"; then
          echo "installed runtime dependency is unresolved: $artifact -> $dependency" >&2
          exit 1
        fi
      done <<<"$linkage"
    done

    workerRuntimePaths="$(runtime_paths "$out/bin/ortools-worker")"
    case "$workerRuntimePaths" in
      *"$out/lib"*) ;;
      *)
        echo "installed worker does not search the packaged runtime directory" >&2
        printf '%s\n' "$workerRuntimePaths" >&2
        exit 1
        ;;
    esac

    echo "installed worker startup and runtime-closure checks passed"

    runHook postInstallCheck
  '';

  passthru = {
    inherit contract;
    sourceContractApproved = true;
    nixWorkerArtifactBuildable = true;
    solverManifestAvailable = false;
    sbomAvailable = false;
    licensePayloadAvailable = false;
    packagingAvailable = false;
    backendAvailable = false;
    releaseReady = false;
  };

  meta = {
    description = "Pinned OR-Tools CP-SAT worker for eutheto";
    homepage = "https://github.com/google/or-tools";
    license = with lib.licenses; [
      asl20
      bsd3
      bsdOriginal
      mit
      mpl20
      zlib
    ];
    mainProgram = "ortools-worker";
    platforms = supportedSystems;
  };
}
