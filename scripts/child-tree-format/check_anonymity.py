#!/usr/bin/env python3
"""Check the child tree format research files for identifying text.

Exit codes: 0 clean, 1 violation found, 2 could not judge. A violation prints
the file, line and pattern kind only, so private terms never reach a log.

The denylist and the raw snapshot live outside the repository and are always
passed as arguments. Patterns below use a character class so that this file
does not match itself.
"""

import argparse
import json
import re
import sys
from pathlib import Path

sys.dont_write_bytecode = True

REPO_ROOT = Path(__file__).resolve().parents[2]
TARGET_DIRS = ("docs/research/child-tree-format", "scripts/child-tree-format")

PUBLIC_PATTERNS = (
    ("unix-home-path", re.compile(r"/hom[e]/|/User[s]/", re.I)),
    ("drive-path", re.compile(r"(?<![a-z0-9])[a-z]:[\\/](?![/])", re.I)),
    ("email", re.compile(r"[\w.+-]+@[\w-]+\.[a-z][\w.-]*", re.I)),
    (
        "private-ip",
        re.compile(
            r"(?<![\d.])(?:10\.\d{1,3}|172\.(?:1[6-9]|2\d|3[01])|192\.168)\.\d{1,3}\.\d{1,3}(?![\d.])"
        ),
    ),
    ("session-token-name", re.compile(r"TASTY_SESSI[O]N_TOKEN", re.I)),
    ("session-field", re.compile(r"sessi[o]n_id|thre[a]d_id", re.I)),
    ("request-number", re.compile(r"\bR-\d+", re.I)),
)

# Snapshot terms too common to mean anything in these files, with the reason.
COMMON_TERMS = {
    "home": "the home path shape is a public pattern already",
    "work": "ordinary word and part of the pseudonym path",
    "workspace": "ordinary word in the product vocabulary",
    "lane": "the pseudonym vocabulary uses it",
    "tasty": "the name of this public repository",
    "conductor": "ordinary word in the product vocabulary",
    ".git": "the name git gives its own folder",
}
MIN_TERM_LENGTH = 3

# Git writes these header lines for every commit. They are not research content.
GIT_METADATA = re.compile(r"^(?:Author:|Commit:|AuthorDate:|CommitDate:|\s+Co-Authored-By:)", re.I)


def snapshot_terms(snapshot):
    """Return the terms a raw snapshot forbids in published files."""
    terms = set()
    for node in snapshot.get("nodes", []):
        nickname = node.get("nickname")
        if nickname:
            terms.add(nickname)
            # Identifier-shaped words carry the request content on their own.
            terms.update(w for w in nickname.split() if "_" in w)
        cwd = node.get("cwd")
        if cwd:
            terms.update(p for p in re.split(r"[\\/]", cwd) if p)
        for key in ("repo_key", "external_parent"):
            if node.get(key):
                terms.update(p for p in re.split(r"[\\/]", str(node[key])) if p)
    return {t.lower() for t in terms if len(t) >= MIN_TERM_LENGTH and t.lower() not in COMMON_TERMS}


def read_denylist(path):
    lines = Path(path).read_text(encoding="utf-8").splitlines()
    return {s.strip().lower() for s in lines if s.strip() and not s.lstrip().startswith("#")}


def string_values(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for k, v in value.items():
            yield k
            yield from string_values(v)
    elif isinstance(value, list):
        for v in value:
            yield from string_values(v)


def line_views(name, line):
    """Yield the line itself and, for JSON lines, its decoded strings.

    A response body in a JSON line can hide a term behind an escape sequence.
    """
    yield line
    if name.endswith((".json", ".jsonl")):
        try:
            decoded = json.loads(line)
        except ValueError:
            return
        yield from string_values(decoded)


def scan_text(name, text, terms, stdin_mode=False):
    violations = []
    skipped = 0
    for number, line in enumerate(text.splitlines(), 1):
        if stdin_mode and GIT_METADATA.match(line):
            skipped += 1
            continue
        found = set()
        for view in line_views(name, line):
            for kind, pattern in PUBLIC_PATTERNS:
                if pattern.search(view):
                    found.add(kind)
            lowered = view.lower()
            if any(term in lowered for term in terms):
                found.add("private-term")
        violations.extend((name, number, kind) for kind in sorted(found))
    return violations, skipped


def target_files(root):
    files = []
    for rel in TARGET_DIRS:
        base = root / rel
        if base.is_dir():
            files.extend(
                p for p in sorted(base.rglob("*"))
                if p.is_file() and "__pycache__" not in p.parts
            )
    return files


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--denylist", help="file with one private term per line")
    parser.add_argument("--snapshot", help="raw snapshot file to read extra terms from")
    parser.add_argument("--public-only", action="store_true", help="skip the private term check")
    parser.add_argument("--stdin", action="store_true", help="scan standard input, such as git log -p")
    parser.add_argument("--root", default=str(REPO_ROOT), help="tree to scan")
    args = parser.parse_args(argv)

    terms = set()
    if args.public_only:
        print("private term check: not run (--public-only)")
    else:
        if not args.denylist or not args.snapshot:
            print("cannot judge: --denylist and --snapshot are required", file=sys.stderr)
            return 2
        try:
            terms |= read_denylist(args.denylist)
            denylist_count = len(terms)
            terms |= snapshot_terms(json.loads(Path(args.snapshot).read_text(encoding="utf-8")))
        except (OSError, ValueError) as error:
            print(f"cannot judge: could not read an input ({type(error).__name__})", file=sys.stderr)
            return 2
        if not denylist_count:
            print("cannot judge: the denylist has no terms", file=sys.stderr)
            return 2

    violations = []
    if args.stdin:
        text = sys.stdin.read()
        if not text.strip():
            print("cannot judge: standard input is empty", file=sys.stderr)
            return 2
        violations, skipped = scan_text("<stdin>", text, terms, stdin_mode=True)
        print(f"scanned stdin: {len(text.splitlines())} lines, {skipped} git metadata lines skipped")
    else:
        root = Path(args.root)
        files = target_files(root)
        if not files:
            print("cannot judge: no target files", file=sys.stderr)
            return 2
        for path in files:
            text = path.read_bytes().decode("utf-8", errors="replace")
            found, _ = scan_text(str(path.relative_to(root)), text, terms)
            violations.extend(found)
        print(f"scanned files: {len(files)}")
    print(f"public patterns: {len(PUBLIC_PATTERNS)}, private terms: {len(terms)}")

    for name, number, kind in violations:
        print(f"{name}:{number}: {kind}")
    print(f"violations: {len(violations)}")
    return 1 if violations else 0


if __name__ == "__main__":
    sys.exit(main())
