#!/usr/bin/env python3
"""Turn a raw snapshot into the published tree. The raw file and the map stay local.

Only id, parents, nickname, state, cwd and last_report_at survive. Each of them
is replaced by a pseudonym except the state. Milliseconds are floored to
seconds before the clock shift.
"""

import argparse
import json
import sys
from pathlib import Path

sys.dont_write_bytecode = True

from common import dumps, ordered, rng, shift_seconds, validate_nodes, vocabulary_nicknames


def anonymize(snapshot):
    """Return (published tree, local map)."""
    raw = snapshot["nodes"]
    gen = rng("tree", "real")
    numbers = gen.sample(range(100, 1000), len(raw))
    ids = {n["surface"]: f"n{number}" for n, number in zip(raw, numbers)}

    names = vocabulary_nicknames(len(raw))
    gen.shuffle(names)
    nicknames = {}
    for node in raw:
        if node.get("nickname"):
            nicknames[node["surface"]] = names.pop()

    repo_keys = sorted({n["repo_key"] for n in raw if n.get("repo_key")})
    gen.shuffle(repo_keys)
    letters = "abcdefghijklmnopqrstuvwxyz"
    repo_letter = {key: letters[i] for i, key in enumerate(repo_keys)}
    unknown_letter = letters[len(repo_keys)]
    counters = {}
    cwd_map = {}

    def pseudonym(node):
        # A node whose path cannot be read gets its own pseudonym under one extra repository.
        source = node.get("cwd") or ("unknown", node["surface"])
        if source not in cwd_map:
            letter = repo_letter.get(node.get("repo_key"), unknown_letter)
            counters[letter] = counters.get(letter, 0) + 1
            cwd_map[source] = f"/work/repo-{letter}/wt-{counters[letter]:02d}"
        return cwd_map[source]

    shuffled = list(raw)
    gen.shuffle(shuffled)
    cwds = {n["surface"]: pseudonym(n) for n in shuffled}

    seconds = [
        None if n.get("last_state_report_at") is None else int(n["last_state_report_at"]) // 1000
        for n in raw
    ]
    times = dict(zip((n["surface"] for n in raw), shift_seconds(seconds)))

    nodes = [
        {
            "id": ids[n["surface"]],
            "parents": [ids[n["parent"]]] if n.get("parent") in ids else [],
            "nickname": nicknames.get(n["surface"]),
            "state": n["state"],
            "cwd": cwds[n["surface"]],
            "last_report_at": times[n["surface"]],
        }
        for n in raw
    ]
    validate_nodes(nodes)
    tree = {
        "tree": "real",
        "capture": {
            "external_parent_roots": sum(1 for n in raw if n.get("external_parent")),
            "cwd_unknown_nodes": sum(1 for n in raw if not n.get("cwd")),
            "surfaces_outside_forest": snapshot.get("registry_surfaces_outside_forest", 0),
            "state_sources": snapshot.get("state_sources", {}),
        },
        "nodes": ordered(nodes),
    }
    local_map = {
        "ids": {str(k): v for k, v in ids.items()},
        "nicknames": {str(k): v for k, v in nicknames.items()},
        "cwds": {str(k): v for k, v in cwd_map.items()},
    }
    return tree, local_map


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--snapshot", required=True, help="raw snapshot from capture_snapshot.py")
    parser.add_argument("--map-out", required=True, help="local map output, outside the repository")
    parser.add_argument("--out", required=True, help="published tree file")
    args = parser.parse_args(argv)
    snapshot = json.loads(Path(args.snapshot).read_text(encoding="utf-8"))
    tree, local_map = anonymize(snapshot)
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    Path(args.out).write_text(dumps(tree), encoding="utf-8")
    Path(args.map_out).write_text(json.dumps(local_map, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(f"published nodes={len(tree['nodes'])}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
