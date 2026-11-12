#!/usr/bin/env python3
"""Check that the decision rules section still has its first committed bytes.

With --raw it also checks the order rules for raw result files: every line names
a rules version that exists on the integration branch, and the first rules commit
came first. The integration branch is --main-ref when given, else the local
develop, else origin/develop. When none of them exists the check cannot judge and
names --main-ref. A shallow history cannot prove that a commit is not an ancestor,
so it is reported as "cannot judge" instead of as a missing ancestor.

Exit codes: 0 same in the file and in every committed version, 1 changed, 2 could not judge.
"""

import argparse
import hashlib
import json
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

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


def _git(root, *args):
    return subprocess.run(
        ["git", "-C", str(root), *args], capture_output=True, text=True, check=True,
    ).stdout.strip()


DEFAULT_REFS = ("develop", "origin/develop")


def _ref_exists(root, ref):
    done = subprocess.run(
        ["git", "-C", str(root), "rev-parse", "-q", "--verify", f"{ref}^{{commit}}"], capture_output=True,
    )
    return done.returncode == 0


def resolve_main_ref(root=REPO_ROOT, main_ref=None):
    """Return the integration ref to compare with, or raise ValueError when it does not exist.

    HEAD is never used in its place: a rules commit on HEAD proves nothing about the integration branch.
    """
    if main_ref is not None:
        if not _ref_exists(root, main_ref):
            raise ValueError(f"the ref given with --main-ref does not exist: {main_ref}")
        return main_ref
    for ref in DEFAULT_REFS:
        if _ref_exists(root, ref):
            return ref
    raise ValueError("neither develop nor origin/develop exists; pass the integration ref with --main-ref")


def _is_shallow(root):
    return _git(root, "rev-parse", "--is-shallow-repository") == "true"


def _on_main(commit, root, main_ref):
    """Return whether commit is an ancestor of main_ref, or raise ValueError when git cannot tell."""
    done = subprocess.run(["git", "-C", str(root), "merge-base", "--is-ancestor", commit, main_ref], capture_output=True)
    if done.returncode == 0:
        return True
    if done.returncode == 1:
        if _is_shallow(root):
            raise ValueError(
                f"the history is shallow, so it cannot prove whether {commit} is an ancestor of {main_ref}; "
                "fetch the full history (git fetch --unshallow)"
            )
        return False
    raise ValueError(f"git could not compare {commit} with {main_ref}")


def rules_provenance(root=REPO_ROOT, main_ref=None):
    """Return the rules commit and blob of a run, or raise ValueError when they cannot be used."""
    main_ref = resolve_main_ref(root, main_ref)
    try:
        commit = _git(root, "log", "-1", "--format=%H", "--", RULES_REL)
        if not commit:
            raise ValueError("the rules file has no commit")
        blob = _git(root, "rev-parse", f"{commit}:{RULES_REL}")
        working = _git(root, "hash-object", RULES_REL)
    except subprocess.CalledProcessError as error:
        raise ValueError("git could not read the rules file") from error
    if working != blob:
        raise ValueError("the rules file has uncommitted changes")
    if not _on_main(commit, root, main_ref):
        raise ValueError(f"the rules commit is not an ancestor of {main_ref}")
    if section_hash(committed_bytes(commit, root)) != DECISION_SHA256:
        raise ValueError("the decision rules section differs from the first commit")
    return {"rules_commit": commit, "rules_blob": blob}


def raw_problems(rows, root=REPO_ROOT, main_ref=None, raw_files=()):
    """Return the order rule violations of raw lines as (kind, value) pairs."""
    main_ref = resolve_main_ref(root, main_ref)
    problems = []
    pairs = {(r.get("rules_commit"), r.get("rules_blob")) for r in rows}
    for commit, blob in sorted(pairs, key=str):
        if not commit or not blob:
            problems.append(("missing_rules_fields", str(commit)))
            continue
        try:
            actual = _git(root, "rev-parse", f"{commit}:{RULES_REL}")
        except subprocess.CalledProcessError:
            problems.append(("unknown_rules_commit", commit))
            continue
        if actual != blob:
            problems.append(("blob_differs", commit))
        if not _on_main(commit, root, main_ref):
            problems.append(("not_on_main", commit))
    first = committed_versions(root)[-1]
    started = [r["started_at"] for r in rows if r.get("started_at")]
    if started:
        committed_at = int(_git(root, "log", "-1", "--format=%ct", first))
        moment = datetime.strptime(min(started), "%Y-%m-%dT%H:%M:%S.%fZ").replace(tzinfo=timezone.utc)
        if committed_at >= moment.timestamp():
            problems.append(("rules_committed_after_first_call", first))
    for raw in raw_files:
        # A file outside the repository has no history to compare.
        resolved = Path(raw).resolve()
        if not resolved.is_relative_to(Path(root).resolve()):
            continue
        added = _git(root, "log", "--diff-filter=A", "--format=%H", "--", str(resolved)).split()
        if added and not _on_main(first, root, added[-1]):
            problems.append(("rules_not_ancestor_of_raw", raw))
    return problems


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--raw", nargs="*", default=[], help="raw result files to check")
    parser.add_argument("--main-ref", help="integration ref (default: develop, else origin/develop)")
    args = parser.parse_args(argv)
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
    if args.raw:
        rows = []
        try:
            for name in args.raw:
                rows += [json.loads(line) for line in Path(name).read_text(encoding="utf-8").splitlines() if line.strip()]
            problems = raw_problems(rows, REPO_ROOT, args.main_ref, args.raw)
        except (OSError, ValueError, subprocess.CalledProcessError) as error:
            print(f"cannot judge: {error}", file=sys.stderr)
            return 2
        print(f"raw lines checked: {len(rows)}, order problems: {len(problems)}")
        for kind, value in problems:
            print(f"{kind}: {value}")
        if not rows:
            print("cannot judge: no raw lines", file=sys.stderr)
            return 2
        return 1 if problems else 0
    return 0


if __name__ == "__main__":
    sys.exit(main())
