#!/usr/bin/env python3

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from verify_targets import HarnessError, REQUIRED_SEEDS, REQUIRED_TARGETS, verify


class VerifyTargetsTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.fuzz_dir = Path(self.temporary.name)
        (self.fuzz_dir / "fuzz_targets").mkdir()
        (self.fuzz_dir / "corpus").mkdir()
        bins = []
        for target in REQUIRED_TARGETS:
            source = f"fuzz_targets/{target}.rs"
            bins.append(
                "\n".join(
                    [
                        "[[bin]]",
                        f'name = "{target}"',
                        f'path = "{source}"',
                        "test = false",
                        "doc = false",
                        "bench = false",
                    ]
                )
            )
            (self.fuzz_dir / source).touch()
            corpus = self.fuzz_dir / "corpus" / target
            corpus.mkdir()
            seeds = REQUIRED_SEEDS.get(target) or {"smoke"}
            for seed in seeds:
                (corpus / seed).write_bytes(b"seed")
        (self.fuzz_dir / "Cargo.toml").write_text("\n\n".join(bins), encoding="utf-8")

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def test_complete_exact_harness_passes(self) -> None:
        expected_seeds = sum(
            len(REQUIRED_SEEDS.get(target) or {"smoke"}) for target in REQUIRED_TARGETS
        )
        self.assertEqual(
            verify(self.fuzz_dir),
            (len(REQUIRED_TARGETS), expected_seeds),
        )

    def test_missing_required_target_fails(self) -> None:
        (self.fuzz_dir / "fuzz_targets" / "migration_chain.rs").unlink()
        with self.assertRaises(HarnessError):
            verify(self.fuzz_dir)

    def test_manifest_omission_fails_even_when_source_exists(self) -> None:
        manifest = (self.fuzz_dir / "Cargo.toml").read_text(encoding="utf-8")
        start = manifest.index('[[bin]]\nname = "bundle_remap"')
        (self.fuzz_dir / "Cargo.toml").write_text(manifest[:start], encoding="utf-8")
        with self.assertRaises(HarnessError):
            verify(self.fuzz_dir)

    def test_unexpected_target_fails(self) -> None:
        (self.fuzz_dir / "fuzz_targets" / "unreviewed.rs").touch()
        with self.assertRaises(HarnessError):
            verify(self.fuzz_dir)

    def test_missing_required_named_seed_fails(self) -> None:
        (self.fuzz_dir / "corpus" / "bundle_remap" / "cross-scenario-shared-reference").unlink()
        with self.assertRaises(HarnessError):
            verify(self.fuzz_dir)


if __name__ == "__main__":
    unittest.main()
