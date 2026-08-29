{ pkgs, src }:
{
  required-files = pkgs.runCommand "eutheto-required-files" { } ''
    set -eu
    for path in \
      .envrc \
      AGENTS.md \
      Cargo.lock \
      Cargo.toml \
      CODE_OF_CONDUCT.md \
      CONTRIBUTING.md \
      DCO.md \
      GOVERNANCE.md \
      Justfile \
      LICENSE \
      NOTICE \
      README.md \
      SECURITY.md \
      THIRD_PARTY_LICENSES \
      THIRD_PARTY_NOTICES.md \
      TRADEMARKS.md \
      flake.lock \
      flake.nix \
      package.json \
      pnpm-lock.yaml \
      pnpm-workspace.yaml \
      protocol/solver-worker.proto \
      protocol/version.json \
      rust-toolchain.toml \
      nix/checks.nix \
      nix/dev-shell.nix \
      nix/dev-shell-welcome.sh \
      nix/eutheto-cli.nix \
      nix/ortools-worker.nix \
      nix/packages.nix \
      nix/release-shell.nix \
      nix/release-tooling.nix \
      nix/source-filter.nix \
      nix/tooling.nix
    do
      if ! test -e "${src}/$path"; then
        echo "required repository path is missing: $path" >&2
        exit 1
      fi
    done
    touch "$out"
  '';

  exact-pins-and-json = pkgs.runCommand "eutheto-exact-pins-and-json" { } ''
    set -eu
    ${pkgs.python3}/bin/python3 - "${src}" <<'PY'
    import json
    import pathlib
    import sys
    import tomllib

    root = pathlib.Path(sys.argv[1])

    json_files = sorted(root.rglob("*.json"))
    if not json_files:
        raise SystemExit("repository contains no JSON inputs")
    for path in json_files:
        with path.open("rb") as stream:
            json.load(stream)

    with (root / "rust-toolchain.toml").open("rb") as stream:
        toolchain = tomllib.load(stream)["toolchain"]
    assert toolchain["channel"] == "1.97.1"
    assert toolchain["profile"] == "minimal"
    assert sorted(toolchain["components"]) == sorted([
        "cargo",
        "clippy",
        "rust-analyzer",
        "rust-src",
        "rustfmt",
    ])

    with (root / "Cargo.toml").open("rb") as stream:
        cargo = tomllib.load(stream)
    assert cargo["workspace"]["members"] == [
        "crates/eutheto-types",
        "crates/eutheto-core",
        "crates/eutheto-cli",
        "apps/desktop/src-tauri",
        "xtask",
    ]
    assert cargo["workspace"]["package"]["rust-version"] == "1.97.1"

    with (root / "package.json").open(encoding="utf-8") as stream:
        package = json.load(stream)
    assert package["private"] is True
    assert package["packageManager"] == "pnpm@11.24.0+sha512.bd27e345e976dcb0be0b7a1228217b049a817e21b1f355c90dbe7dc46671895a8bc1e6d06c24554505ea93ea0b45f489a27ec1bfbc8de6a9659fca0f16fa0000"
    assert package["engines"] == {
        "node": ">=24 <25",
        "pnpm": ">=11 <12",
    }
    PY
    touch "$out"
  '';

  envrc = pkgs.runCommand "eutheto-envrc" { } ''
    set -eu
    printf 'use flake .\n' > "$TMPDIR/expected-envrc"
    if ! ${pkgs.diffutils}/bin/cmp -s "$TMPDIR/expected-envrc" "${src}/.envrc"; then
      echo ".envrc must contain exactly: use flake ." >&2
      exit 1
    fi
    touch "$out"
  '';

  legal-baseline = pkgs.runCommand "eutheto-legal-baseline" { } ''
    set -eu
    ${pkgs.python3}/bin/python3 - "${src}" <<'PY'
    import pathlib
    import sys

    root = pathlib.Path(sys.argv[1])
    required = [
        "CODE_OF_CONDUCT.md",
        "CONTRIBUTING.md",
        "DCO.md",
        "GOVERNANCE.md",
        "LICENSE",
        "NOTICE",
        "SECURITY.md",
        "THIRD_PARTY_NOTICES.md",
        "TRADEMARKS.md",
    ]
    for relative in required:
        path = root / relative
        if not path.is_file() or not path.read_text(encoding="utf-8").strip():
            raise SystemExit(f"legal baseline file is missing or empty: {relative}")
    if not (root / "THIRD_PARTY_LICENSES").is_dir():
        raise SystemExit("THIRD_PARTY_LICENSES must be a directory")
    license_text = (root / "LICENSE").read_text(encoding="utf-8")
    if "Apache License" not in license_text or "Version 2.0, January 2004" not in license_text:
        raise SystemExit("LICENSE is not the Apache-2.0 license text")
    dco_text = (root / "DCO.md").read_text(encoding="utf-8")
    if "Developer Certificate of Origin" not in dco_text or "1.1" not in dco_text:
        raise SystemExit("DCO.md does not contain the DCO 1.1 baseline")
    PY
    touch "$out"
  '';

  protocol-contract = pkgs.runCommand "eutheto-protocol-contract" { } ''
    set -eu
    ${pkgs.python3}/bin/python3 - "${src}" <<'PY'
    import json
    import pathlib
    import sys

    root = pathlib.Path(sys.argv[1])
    protocol = root / "protocol"
    with (protocol / "version.json").open(encoding="utf-8") as stream:
        manifest = json.load(stream)
    assert manifest["protocol"] == "eutheto.solver-worker"
    assert manifest["wire_version"] == 1
    assert manifest["compatibility"]["accepted_wire_versions"] == [1]
    assert manifest["compatibility"]["unknown_version_action"] == "bounded_error_then_close"
    framing = manifest["framing"]
    assert framing["length_prefix_bytes"] == 4
    assert framing["length_prefix_order"] == "big-endian"
    assert 0 < framing["min_payload_bytes"] <= framing["max_payload_bytes"]

    schema = (protocol / "solver-worker.proto").read_text(encoding="utf-8")
    if 'syntax = "proto3";' not in schema or "package eutheto.solver.worker.v1;" not in schema:
        raise SystemExit("solver-worker.proto does not declare the approved Phase-00 protocol package")

    golden = protocol / "golden"
    required = {
        "handshake-request",
        "handshake-result",
        "health-request",
        "health-result",
        "unsupported-version-request",
        "unsupported-version-error",
    }
    json_stems = {path.stem for path in golden.glob("*.json")}
    frame_stems = {
        path.name.removesuffix(".frame.hex")
        for path in golden.glob("*.frame.hex")
    }
    if not required.issubset(json_stems & frame_stems):
        missing = sorted(required - (json_stems & frame_stems))
        raise SystemExit(f"protocol golden pairs are missing: {missing}")
    if json_stems != frame_stems:
        raise SystemExit("every protocol golden JSON file must have exactly one frame hex peer")

    for stem in sorted(json_stems):
        with (golden / f"{stem}.json").open(encoding="utf-8") as stream:
            json.load(stream)
        encoded = "".join((golden / f"{stem}.frame.hex").read_text(encoding="ascii").split())
        try:
            frame = bytes.fromhex(encoded)
        except ValueError as error:
            raise SystemExit(f"invalid hexadecimal protocol frame {stem}: {error}") from error
        if len(frame) < 5:
            raise SystemExit(f"protocol frame is too short: {stem}")
        payload_length = int.from_bytes(frame[:4], byteorder="big")
        if payload_length != len(frame) - 4:
            raise SystemExit(f"protocol frame length prefix does not match payload: {stem}")
    PY
    touch "$out"
  '';

  workspace-manifests = pkgs.runCommand "eutheto-workspace-manifests" { } ''
    set -eu
    export CARGO_HOME="$TMPDIR/cargo-home"
    export CARGO_NET_OFFLINE=true
    ${pkgs.cargo}/bin/cargo metadata \
      --format-version 1 \
      --locked \
      --manifest-path "${src}/Cargo.toml" \
      --no-deps \
      > "$TMPDIR/cargo-metadata.json"
    ${pkgs.jq}/bin/jq -e '
      (.workspace_members | length) == 5 and
      ([.packages[].name] | sort) == ([
        "eutheto-cli",
        "eutheto-core",
        "eutheto-desktop",
        "eutheto-types",
        "xtask"
      ] | sort)
    ' "$TMPDIR/cargo-metadata.json" > /dev/null

    ${pkgs.yq-go}/bin/yq eval -o=json "${src}/pnpm-workspace.yaml" > "$TMPDIR/pnpm-workspace.json"
    ${pkgs.jq}/bin/jq -e '.packages == ["apps/*", "packages/*"]' "$TMPDIR/pnpm-workspace.json" > /dev/null
    ${pkgs.yq-go}/bin/yq eval -o=json "${src}/pnpm-lock.yaml" > "$TMPDIR/pnpm-lock.json"
    ${pkgs.jq}/bin/jq -e 'type == "object" and has("lockfileVersion")' "$TMPDIR/pnpm-lock.json" > /dev/null
    touch "$out"
  '';

  nix-format = pkgs.runCommand "eutheto-nix-format" { } ''
    set -eu
    for path in "${src}/flake.nix" "${src}"/nix/*.nix; do
      ${pkgs.nixfmt}/bin/nixfmt --check "$path"
    done
    touch "$out"
  '';
}
