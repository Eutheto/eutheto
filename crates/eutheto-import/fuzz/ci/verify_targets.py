#!/usr/bin/env python3
"""Fail unless the complete, exact Phase-01/02 fuzz harness is present."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import tomllib

REQUIRED_TARGETS = (
    "scenario_envelope",
    "bundle",
    "migration_chain",
    "bundle_remap",
    "planning_ir",
    "integer_expression",
    "projection",
    "component_graph",
)
REQUIRED_SEEDS = {
    "migration_chain": {"historical-to-current"},
    "bundle_remap": {
        "cross-scenario-shared-reference",
        "canonical-result-remap",
        "malformed-declared-list",
        "tombstone-revision-floor",
    },
    "planning_ir": {"canonical-empty-problem"},
    "integer_expression": {"duplicate-terms-and-bounds"},
    "projection": {"complete-candidate"},
    "component_graph": {"projection-joined"},
}


class HarnessError(RuntimeError):
    pass


def verify(fuzz_dir: Path) -> tuple[int, int]:
    manifest_path = fuzz_dir / "Cargo.toml"
    if not manifest_path.is_file():
        raise HarnessError(f"missing fuzz manifest: {manifest_path}")

    with manifest_path.open("rb") as manifest_file:
        manifest = tomllib.load(manifest_file)
    declared_bins = manifest.get("bin", [])
    expected = {
        target: f"fuzz_targets/{target}.rs" for target in REQUIRED_TARGETS
    }
    if len(declared_bins) != len(expected) or any(
        not isinstance(binary, dict) for binary in declared_bins
    ):
        raise HarnessError(
            f"fuzz manifest must declare exactly {len(expected)} binary targets"
        )
    declared = {binary.get("name"): binary.get("path") for binary in declared_bins}
    if len(declared) != len(declared_bins) or declared != expected:
        raise HarnessError(
            f"fuzz manifest target set must be exactly {expected!r}, found {declared!r}"
        )
    for binary in declared_bins:
        if any(binary.get(setting) is not False for setting in ("test", "doc", "bench")):
            raise HarnessError(
                f"fuzz target {binary.get('name')!r} must disable test, doc, and bench"
            )

    target_dir = fuzz_dir / "fuzz_targets"
    discovered = {
        source.stem
        for source in target_dir.glob("*.rs")
        if source.is_file() and source.name != "mod.rs"
    }
    if discovered != set(REQUIRED_TARGETS):
        missing = sorted(set(REQUIRED_TARGETS) - discovered)
        unexpected = sorted(discovered - set(REQUIRED_TARGETS))
        raise HarnessError(
            f"fuzz source target set mismatch; missing={missing!r}, unexpected={unexpected!r}"
        )

    seed_count = 0
    corpus_dir = fuzz_dir / "corpus"
    for target in REQUIRED_TARGETS:
        target_corpus = corpus_dir / target
        seeds = {seed.name for seed in target_corpus.iterdir() if seed.is_file()} if target_corpus.is_dir() else set()
        if not seeds:
            raise HarnessError(f"fuzz target {target!r} has no checked-in seed corpus")
        required_seeds = REQUIRED_SEEDS.get(target, set())
        missing_seeds = sorted(required_seeds - seeds)
        if missing_seeds:
            raise HarnessError(
                f"fuzz target {target!r} is missing required named seeds {missing_seeds!r}"
            )
        seed_count += len(seeds)

    return len(REQUIRED_TARGETS), seed_count


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("fuzz_dir", type=Path)
    parser.add_argument("--github-output", type=Path)
    args = parser.parse_args()
    try:
        target_count, seed_count = verify(args.fuzz_dir)
    except (HarnessError, OSError, tomllib.TOMLDecodeError) as error:
        print(f"error: {error}", file=os.sys.stderr)
        return 1

    print(f"verified {target_count} required fuzz targets and {seed_count} seed files")
    if args.github_output is not None:
        with args.github_output.open("a", encoding="utf-8") as output:
            output.write(f"target_count={target_count}\n")
            output.write(f"corpus_count={seed_count}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
