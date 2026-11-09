#!/usr/bin/env python3
"""Check that the decision rules section still has its first committed bytes.

Exit codes: 0 same in the file and in every committed version, 1 changed, 2 could not judge.
"""

import argparse
import hashlib
import re
import subprocess
import sys

sys.dont_write_bytecode = True

from common import REPO_ROOT, RULES_PATH, RULES_REL

DECISION_SHA256 = "629785d470081fcf4f8f159a44021ffcc8f49db441765a09da83265a31881500"
SECTION = re.compile(rb"<!-- decision-rules:begin -->\n(.*)<!-- decision-rules:end -->", re.S)


def section_hash(data):
    """Return the sha256 of the bytes between the marker lines, or None without markers."""
    matches = SECTION.findall(data)
    if len(matches) != 1:
        return None
    return hashlib.sha256(matches[0]).hexdigest()


def committed_versions(root=REPO_ROOT):
    log = subprocess.run(
        ["git", "-C", str(root), "log", "--format=%H", "--", RULES_REL],
        capture_output=True, text=True, check=True,
    )
    return log.stdout.split()


def committed_bytes(commit, root=REPO_ROOT):
    return subprocess.run(
        ["git", "-C", str(root), "show", f"{commit}:{RULES_REL}"],
        capture_output=True, check=True,
    ).stdout


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args(argv)
    current = section_hash(RULES_PATH.read_bytes())
    if current is None:
        print("cannot judge: the markers are missing", file=sys.stderr)
        return 2
    if current != DECISION_SHA256:
        print("changed: the working file differs from the first commit")
        return 1
    try:
        commits = committed_versions()
        changed = [c for c in commits if section_hash(committed_bytes(c)) != DECISION_SHA256]
    except subprocess.CalledProcessError:
        print("cannot judge: git could not read the history", file=sys.stderr)
        return 2
    if not commits:
        print("cannot judge: the file has no commits", file=sys.stderr)
        return 2
    print(f"decision rules sha256 {current}, committed versions checked: {len(commits)}")
    if changed:
        for commit in changed:
            print(f"changed in {commit}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
