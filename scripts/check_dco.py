#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Validate repository DCO sign-offs for an explicit Git commit range."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from typing import Any


REPOSITORY = Path(
    os.environ.get("EUTHETO_DCO_REPOSITORY", Path(__file__).resolve().parents[1])
).resolve()
FIXTURES = Path(
    os.environ.get(
        "EUTHETO_DCO_FIXTURES",
        REPOSITORY / "tests" / "security" / "fixtures" / "dco_cases.json",
    )
).resolve()
OBJECT_ID = re.compile(r"(?:[0-9a-fA-F]{40}|[0-9a-fA-F]{64})\Z")
TRAILER = re.compile(r"([A-Za-z0-9-]+):[ \t]*(\S.*)\Z")
SIGN_OFF = re.compile(
    r"Signed-off-by:[ \t]+([^<>\r\n]+?)[ \t]+"
    r"<([^<>\s@]+@[^<>\s@]+)>[ \t]*\Z",
    re.IGNORECASE,
)


class DcoError(Exception):
    """A closed DCO validation failure suitable for concise CI output."""


def git(*arguments: str) -> bytes:
    environment = os.environ.copy()
    environment.update({"LC_ALL": "C.UTF-8", "LANG": "C.UTF-8"})
    result = subprocess.run(
        ["git", "--no-replace-objects", "-C", str(REPOSITORY), *arguments],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=environment,
    )
    if result.returncode != 0:
        raise DcoError(f"Git could not validate the requested commit range ({arguments[0]})")
    return result.stdout


def require_commit(object_id: str, label: str) -> str:
    if not OBJECT_ID.fullmatch(object_id):
        raise DcoError(f"{label} must be a full Git commit object ID")

    resolved_bytes = git("rev-parse", "--verify", f"{object_id}^{{commit}}")
    try:
        resolved = resolved_bytes.decode("ascii").strip()
    except UnicodeDecodeError as error:
        raise DcoError(f"{label} did not resolve to a Git commit") from error
    if not OBJECT_ID.fullmatch(resolved) or resolved.lower() != object_id.lower():
        raise DcoError(f"{label} did not resolve to the requested Git commit")
    return resolved.lower()


def last_paragraph(message: str) -> list[str]:
    lines = message.splitlines()
    while lines and not lines[-1].strip():
        lines.pop()
    if not lines:
        return []

    start = len(lines) - 1
    while start and lines[start - 1].strip():
        start -= 1
    return lines[start:]


def footer_entries(message: str) -> list[tuple[str, str, bool]]:
    paragraph = last_paragraph(message)
    entries: list[list[Any]] = []
    for line in paragraph:
        match = TRAILER.fullmatch(line)
        if match:
            entries.append([match.group(1), match.group(2), False])
        elif line.startswith((" ", "\t")) and entries:
            entries[-1][2] = True
        else:
            return []
    return [(key, value, continued) for key, value, continued in entries]


def sign_off_result(author_name: str, author_email: str, message: str) -> tuple[bool, str]:
    entries = footer_entries(message)
    saw_sign_off = False
    saw_matching_sign_off = False
    saw_malformed_sign_off = False

    for key, value, continued in entries:
        if key.lower() != "signed-off-by":
            continue
        saw_sign_off = True
        candidate = SIGN_OFF.fullmatch(f"{key}: {value}")
        if candidate is None or continued:
            saw_malformed_sign_off = True
            continue
        if candidate.group(1) == author_name and candidate.group(2) == author_email:
            saw_matching_sign_off = True

    if saw_malformed_sign_off or (
        not saw_sign_off
        and any(
            line.casefold().startswith("signed-off-by")
            for line in last_paragraph(message)
        )
    ):
        return False, "malformed Signed-off-by trailer"
    if saw_matching_sign_off:
        return True, "matching author sign-off"
    if saw_sign_off:
        return False, "sign-off does not match the commit author name and email"
    return False, "missing Signed-off-by trailer"


def commit_text(commit: str, format_string: str) -> str:
    raw = git("show", "-s", f"--format=format:{format_string}", commit, "--")
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError as error:
        raise DcoError(f"commit {commit[:12]} metadata is not valid UTF-8") from error


def commits_in_range(base: str, head: str) -> list[str]:
    # rev-list emits one fixed-width hexadecimal object ID per line. Free-form
    # author and message fields are fetched independently, so no commit content
    # can be confused with a field or record delimiter.
    raw = git("rev-list", f"{base}..{head}", "--")
    try:
        lines = raw.decode("ascii").splitlines()
    except UnicodeDecodeError as error:
        raise DcoError("Git returned a malformed commit list") from error
    if not lines:
        raise DcoError("the requested base..head range contains no commits")
    if any(not OBJECT_ID.fullmatch(line) for line in lines):
        raise DcoError("Git returned a malformed commit list")
    return [line.lower() for line in lines]


def check_range(base_argument: str, head_argument: str) -> None:
    base = require_commit(base_argument, "base")
    head = require_commit(head_argument, "head")
    failures: list[tuple[str, str]] = []

    # The repository policy requires every commit, so rev-list intentionally
    # includes merge commits rather than exempting them.
    commits = commits_in_range(base, head)
    for commit in commits:
        author_name = commit_text(commit, "%an")
        author_email = commit_text(commit, "%ae")
        message = commit_text(commit, "%B")
        valid, reason = sign_off_result(author_name, author_email, message)
        if not valid:
            failures.append((commit, reason))

    if failures:
        for commit, reason in failures:
            print(f"DCO failure: {commit[:12]}: {reason}", file=sys.stderr)
        raise DcoError(
            f"{len(failures)} of {len(commits)} commits failed repository DCO policy"
        )
    print(f"DCO check passed for {len(commits)} commits in {base}..{head}")


def self_test() -> None:
    try:
        document = json.loads(FIXTURES.read_text(encoding="utf-8"))
        cases = document["cases"]
    except (OSError, json.JSONDecodeError, KeyError, TypeError) as error:
        raise DcoError(f"could not load DCO fixtures from {FIXTURES}") from error

    if not isinstance(cases, list) or not cases:
        raise DcoError("DCO fixtures must contain a non-empty cases list")

    required_names = {
        "valid",
        "multiple",
        "mixed-malformed",
        "missing",
        "mismatched",
        "malformed",
    }
    observed_names: set[str] = set()
    for case in cases:
        if not isinstance(case, dict):
            raise DcoError("each DCO fixture must be an object")
        try:
            name = case["name"]
            expected = case["valid"]
            actual, _ = sign_off_result(
                case["author_name"], case["author_email"], case["message"]
            )
        except (KeyError, TypeError) as error:
            raise DcoError("a DCO fixture has an invalid schema") from error
        if not isinstance(name, str) or not isinstance(expected, bool):
            raise DcoError("a DCO fixture has an invalid name or expected result")
        observed_names.add(name)
        if actual is not expected:
            raise DcoError(
                f"DCO fixture {name!r} returned {actual}, expected {expected}"
            )
        print(f"DCO self-test passed: {name} was {'accepted' if actual else 'rejected'}")

    if observed_names != required_names:
        required = ", ".join(sorted(required_names))
        raise DcoError(f"DCO fixtures must contain exactly these cases: {required}")


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="check every commit in an explicit base..head range for a matching DCO sign-off"
    )
    parser.add_argument("--self-test", action="store_true", help="run repository fixtures")
    parser.add_argument("base", nargs="?", help="full base commit object ID")
    parser.add_argument("head", nargs="?", help="full head commit object ID")
    arguments = parser.parse_args()
    if arguments.self_test:
        if arguments.base is not None or arguments.head is not None:
            parser.error("--self-test does not accept a commit range")
    elif arguments.base is None or arguments.head is None:
        parser.error("both base and head commit object IDs are required")
    return arguments


def main() -> int:
    arguments = parse_arguments()
    try:
        if arguments.self_test:
            self_test()
        else:
            check_range(arguments.base, arguments.head)
    except DcoError as error:
        print(f"DCO check failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
